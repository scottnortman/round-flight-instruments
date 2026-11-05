//! Bounded presentation continuity, separate from measurement validity.
//! Repeated pixels retain their original sample age, session and expiry.
use crate::Snapshot;
use instrument_core::{Instrument, Quality};

pub const GRACE_MS: u64 = 300;
// Includes composition, the pending transfer and invalid-view confirmation.
pub const INVALIDATION_RESERVE_MS: u64 = 100;

fn eligible(s: Snapshot) -> bool {
    s.external && s.external_attitude && s.source_running && !s.imu_bench
        && s.source_session != 0
        && matches!(s.selected, Instrument::Attitude | Instrument::Turn)
        && s.menu.is_none() && s.editor.is_none()
}
/// Repeat immutable, completed pixels into the disjoint drawing buffer.
/// The next real draw must restore its face, not reuse obsolete damage history.
pub fn copy_completed_frame(destination: &mut [u16], completed: &[u16], history: &mut crate::damage::FrameHistory) -> bool {
    if destination.len()!=completed.len() { return false; }
    destination.copy_from_slice(completed);
    *history=crate::damage::FrameHistory::new();
    true
}
pub fn lifetime_ms(snapshot: Snapshot) -> Option<u64> {
    let lifetime=snapshot.numeric_lifetime_ms()?;
    Some(if eligible(snapshot) && snapshot.displayed_quality()==Some(Quality::Valid) {
        lifetime.saturating_add(GRACE_MS)
    } else { lifetime })
}
pub fn repeat_allowed(latest: Snapshot, front: Snapshot, elapsed_ms: u64) -> bool {
    repeat_allowed_with_budget(latest,front,elapsed_ms,INVALIDATION_RESERVE_MS)
}
pub fn repeat_allowed_with_budget(latest: Snapshot, front: Snapshot, elapsed_ms: u64, replacement_ms: u64) -> bool {
    let fields=match latest.selected {
        Instrument::Attitude=>[latest.pitch,latest.roll],
        Instrument::Turn=>[latest.turn_rate,latest.ball],
        _=>return false,
    };
    eligible(latest) && eligible(front)
        && latest.source_session==front.source_session && latest.selected==front.selected
        && front.displayed_quality()==Some(Quality::Valid)
        && latest.displayed_quality()==Some(Quality::Stale)
        && fields.iter().all(|r| matches!(r.quality,Quality::Valid | Quality::Stale))
        && lifetime_ms(front).is_some_and(|ttl|
            elapsed_ms < ttl.saturating_sub(replacement_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use instrument_core::{App, Command};
    fn live(gauge: Instrument) -> Snapshot {
        let mut app=App::new(); app.apply(Command::LevelPreset).unwrap(); app.tick(0);
        app.apply(Command::Select(gauge)).unwrap();
        let mut s=Snapshot::from_app(&app);
        s.external=true; s.external_attitude=true; s.source_running=true;
        s.source_session=7; s.source_sequence=10;
        for r in [&mut s.pitch,&mut s.roll,&mut s.turn_rate,&mut s.ball] { r.age_ms=Some(100); }
        s
    }
    #[test]
    fn presentation_grace_keeps_original_data_deadline_and_does_not_renew_repeats() {
        for gauge in [Instrument::Attitude,Instrument::Turn] {
            let front=live(gauge);
            assert_eq!(front.numeric_lifetime_ms(),Some(100));
            assert_eq!(lifetime_ms(front),Some(400));
            let stale=front.aged(120);
            assert_eq!(stale.displayed_quality(),Some(Quality::Stale));
            assert!(repeat_allowed(stale,front,120));
            let cutoff=lifetime_ms(front).unwrap()-INVALIDATION_RESERVE_MS;
            assert!(repeat_allowed(front.aged(cutoff-1),front,cutoff-1));
            assert!(!repeat_allowed(front.aged(cutoff),front,cutoff));
            assert!(!repeat_allowed_with_budget(front.aged(285),front,285,120));
            assert_eq!(front.source_sequence,10);
            assert_eq!(front.displayed_age_ms(),Some(100));
            assert_eq!(stale.displayed_age_ms(),Some(220));
        }
    }
    #[test]
    fn continuity_never_crosses_contexts_or_masks_explicit_failure_or_local_policy() {
        let front=live(Instrument::Attitude); let stale=front.aged(120);
        for n in 0..7 {
            let mut changed=stale;
            match n {
                0=>changed.source_session+=1,
                1=>changed.selected=Instrument::Turn,
                2=>changed.source_running=false,
                3=>changed.external=false,
                4=>changed.pitch.quality=Quality::Failed,
                5=>changed.roll.quality=Quality::Missing,
                _=>changed.pitch.quality=Quality::TimingFault,
            }
            assert!(!repeat_allowed(changed,front,120));
        }
        let mut local=front; local.external=false; local.external_attitude=false;
        assert_eq!(lifetime_ms(local),local.numeric_lifetime_ms());
        assert!(!repeat_allowed(local.aged(120),local,120));
        let alt=live(Instrument::Altimeter);
        assert_eq!(lifetime_ms(alt),alt.numeric_lifetime_ms());
        let mut menu=stale;
        menu.menu=Some(crate::navigation::View { info: false,
            context:crate::navigation::Context { selected:front.selected,external:true,
                external_attitude:true,imu:false,session:front.source_session },
            generation:1,pressed:None,
        });
        assert!(!repeat_allowed(menu,front,120));
    }
    #[test]
    fn repeated_pixels_and_resume_match_full_reference_across_rotating_buffers() {
        extern crate std;
        use crate::{damage::FrameHistory, render::{CachedRenderer,draw_frame}};
        for gauge in [Instrument::Attitude,Instrument::Turn] {
            let base=live(gauge);
            let mut background=std::vec![0;480*480];
            let mut renderer=CachedRenderer::new(&mut background,480,480).unwrap();
            let mut pixels:[std::vec::Vec<u16>;3]=core::array::from_fn(|_|std::vec![0;480*480]);
            let mut histories:[FrameHistory;3]=core::array::from_fn(|_|FrameHistory::new());
            let mut reference=std::vec![0;480*480];
            for n in 0..12 {
                let slot=n%3;
                if n%4==1 {
                    let completed=pixels[(slot+2)%3].clone();
                    assert!(copy_completed_frame(&mut pixels[slot],&completed,&mut histories[slot]));
                    assert!(pixels[slot]==completed,"held pixels must be identical");
                } else {
                    let mut s=base;
                    s.pitch.value=Some(n as f32*3.-12.); s.roll.value=Some(n as f32*9.-30.);
                    s.turn_rate.value=Some(n as f32-4.); s.ball.value=Some(n as f32/8.-0.5);
                    assert!(renderer.draw_incremental(&mut pixels[slot],&mut histories[slot],s,false));
                    draw_frame(&mut reference,480,480,s,false);
                    assert!(pixels[slot]==reference,"resuming from held pixels left damage");
                }
            }
        }
    }
}
