//! Local Win32 profile editor. No Supervisor, serial transport or simulator API is used.
use aircraft_profiles::{
    preview::{self, Sample},
    store::{self, Store},
    AirspeedLimits, AirspeedScale, Profile, Simulator,
};
use instrument_core::{device::Uuid, Instrument};
use std::{
    cell::{Cell, RefCell},
    io,
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{Com::CoCreateGuid, LibraryLoader::*},
    UI::{
        Controls::{Dialogs::*, BST_CHECKED, BST_UNCHECKED, EM_LIMITTEXT},
        Input::KeyboardAndMouse::EnableWindow,
        WindowsAndMessaging::*,
    },
};

const CLASS: &str = "RoundInstrumentsAircraftProfileEditor";
thread_local! {static ACTIVE:Cell<HWND>=const{Cell::new(null_mut())};}
struct Creation {
    pointer: *mut RefCell<Editor>,
    transferred: bool,
}
const LIST: u16 = 10;
const NEW: u16 = 11;
const DUPLICATE: u16 = 12;
const IMPORT: u16 = 13;
const EXPORT: u16 = 14;
const SAVE: u16 = 15;
const PREVIEW: u16 = 16;
const CLOSE: u16 = 17;
const APPLY: u16 = 18;
const DEFAULTS: u16 = 19;
const NAME: u16 = 30;
const SIM: u16 = 31;
const PACKAGE: u16 = 32;
const VARIANT: u16 = 33;
const ID: u16 = 34;
const AIR_MAX: u16 = 40;
const SCALE: u16 = 41;
const LIMITS: u16 = 42;
const VS0: u16 = 43;
const VS1: u16 = 44;
const VFE: u16 = 45;
const VNO: u16 = 46;
const VNE: u16 = 47;
const REFERENCE: u16 = 48;
const VSI_MAX: u16 = 49;
const GAUGE: u16 = 60;
const SAMPLE_A: u16 = 61;
const SAMPLE_B: u16 = 62;
const LABEL_A: u16 = 63;
const LABEL_B: u16 = 64;
const STATUS: u16 = 70;
const UNITS: u16 = 71;
const GAUGES: [Instrument; 8] = [
    Instrument::Airspeed,
    Instrument::Attitude,
    Instrument::Altimeter,
    Instrument::Heading,
    Instrument::Turn,
    Instrument::Vsi,
    Instrument::Consolidated,
    Instrument::Debug,
];
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn default_root() -> io::Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| io::Error::other("LOCALAPPDATA unavailable"))?,
    )
    .join("RoundFlightInstruments")
    .join("profiles"))
}
fn uuid() -> io::Result<Uuid> {
    let mut id = windows_sys::core::GUID::default();
    // SAFETY: live GUID output. CoCreateGuid does not need a COM apartment.
    if unsafe { CoCreateGuid(&mut id) } < 0 {
        return Err(io::Error::other("Could not generate a profile UUID."));
    }
    Uuid::parse(&format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        id.data1,
        id.data2,
        id.data3,
        id.data4[0],
        id.data4[1],
        id.data4[2],
        id.data4[3],
        id.data4[4],
        id.data4[5],
        id.data4[6],
        id.data4[7]
    ))
    .ok_or_else(|| io::Error::other("Invalid generated profile UUID."))
}
struct Editor {
    hwnd: HWND,
    store: Store,
    catalog: Vec<Profile>,
    draft: Profile,
    revision: Option<u64>,
    dirty: bool,
    standalone: bool,
    loading: bool,
    font: HFONT,
    heading_font: HFONT,
    pixels: Vec<u16>,
    dib: Vec<u32>,
    gauge: Instrument,
    preview_ready: bool,
    preview_stale: bool,
    units: &'static str,
}
impl Editor {
    fn new(root: PathBuf, standalone: bool) -> io::Result<Self> {
        Ok(Self {
            hwnd: null_mut(),
            store: Store::open(root)?,
            catalog: Vec::new(),
            draft: Profile::new(uuid()?),
            revision: None,
            dirty: false,
            standalone,
            loading: false,
            font: null_mut(),
            heading_font: null_mut(),
            pixels: vec![0; 480 * 480],
            dib: vec![0; 480 * 480],
            gauge: Instrument::Airspeed,
            preview_ready: false,
            preview_stale: true,
            units: "",
        })
    }
    unsafe fn ctl(&self, id: u16) -> HWND {
        GetDlgItem(self.hwnd, id as i32)
    }
    #[allow(clippy::too_many_arguments)] // Centralizes the native control/rectangle ABI.
    unsafe fn control(
        &self,
        class: &str,
        label: &str,
        id: u16,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        style: u32,
    ) -> HWND {
        let hwnd = CreateWindowExW(
            if class == "EDIT" || class == "LISTBOX" {
                WS_EX_CLIENTEDGE
            } else {
                0
            },
            wide(class).as_ptr(),
            wide(label).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            x,
            y,
            w,
            h,
            self.hwnd,
            id as usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(hwnd, WM_SETFONT, self.font as usize, 1);
        hwnd
    }
    unsafe fn label(&self, label: &str, x: i32, y: i32, w: i32) {
        self.control("STATIC", label, 0, x, y, w, 22, 0);
    }
    unsafe fn edit(&self, id: u16, x: i32, y: i32, w: i32, max: usize) {
        let c = self.control(
            "EDIT",
            "",
            id,
            x,
            y,
            w,
            28,
            WS_TABSTOP | ES_AUTOHSCROLL as u32,
        );
        SendMessageW(c, EM_LIMITTEXT, max, 0);
    }
    unsafe fn combo(&self, id: u16, labels: &[&str], x: i32, y: i32, w: i32) {
        let c = self.control(
            "COMBOBOX",
            "",
            id,
            x,
            y,
            w,
            220,
            WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
        );
        for item in labels {
            SendMessageW(c, CB_ADDSTRING, 0, wide(item).as_ptr() as isize);
        }
        SendMessageW(c, CB_SETCURSEL, 0, 0);
    }
    unsafe fn setup(&mut self) {
        self.font = CreateFontW(
            -16,
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            wide("Segoe UI").as_ptr(),
        );
        self.heading_font = CreateFontW(
            -25,
            0,
            0,
            0,
            FW_SEMIBOLD as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            wide("Segoe UI").as_ptr(),
        );
        let title = self.control("STATIC", "Aircraft profiles", 0, 22, 18, 520, 36, 0);
        SendMessageW(title, WM_SETFONT, self.heading_font as usize, 1);
        self.control("STATIC","Create aircraft-specific instrument settings and preview all eight screens. Save a profile, then Use on connected board. Manual selection; check tray Status for delivery confirmation.",0,22,61,1150,38,0);
        self.label("Saved profiles", 22, 111, 225);
        self.control(
            "LISTBOX",
            "",
            LIST,
            22,
            138,
            225,
            459,
            WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32,
        );
        for (id, text, x, y, w) in [
            (NEW, "New", 22, 613, 106),
            (DUPLICATE, "Duplicate", 140, 613, 107),
            (IMPORT, "Import…", 22, 655, 106),
            (EXPORT, "Export…", 140, 655, 107),
            (SAVE, "Save profile", 280, 707, 142),
            (PREVIEW, "Update preview", 437, 707, 153),
            (CLOSE, "Close", 605, 707, 75),
            (APPLY, "Use on connected board", 726, 707, 245),
            (DEFAULTS, "Use defaults", 988, 707, 190),
        ] {
            self.control(
                "BUTTON",
                text,
                id,
                x,
                y,
                w,
                34,
                WS_TABSTOP | BS_PUSHBUTTON as u32,
            );
        }
        self.label("Profile name", 280, 111, 400);
        self.edit(NAME, 280, 134, 400, 100);
        self.label("Simulator", 280, 176, 400);
        self.combo(
            SIM,
            &[
                "MSFS 2020",
                "MSFS 2024 (future adapter)",
                "DCS World (future adapter)",
            ],
            280,
            198,
            400,
        );
        self.label("Exact aircraft package / add-on", 280, 239, 400);
        self.edit(PACKAGE, 280, 261, 400, 160);
        self.label("Exact aircraft variant", 280, 302, 400);
        self.edit(VARIANT, 280, 324, 400, 160);
        self.label("Profile UUID / saved revision", 280, 365, 400);
        let id = self.control(
            "EDIT",
            "",
            ID,
            280,
            388,
            400,
            26,
            ES_READONLY as u32 | ES_AUTOHSCROLL as u32 | WS_TABSTOP,
        );
        SendMessageW(id, WM_SETFONT, GetStockObject(DEFAULT_GUI_FONT) as usize, 1);
        self.label("Airspeed full scale (KIAS)", 280, 433, 210);
        self.edit(AIR_MAX, 500, 429, 80, 4);
        self.label("Scale layout", 280, 470, 105);
        self.combo(
            SCALE,
            &["Prototype layout (scaled)", "Linear layout"],
            390,
            466,
            290,
        );
        self.control(
            "BUTTON",
            "Use verified aircraft airspeed limits",
            LIMITS,
            280,
            507,
            400,
            24,
            WS_TABSTOP | BS_AUTOCHECKBOX as u32,
        );
        for (n, (id, label)) in [
            (VS0, "Vs0"),
            (VS1, "Vs1"),
            (VFE, "Vfe"),
            (VNO, "Vno"),
            (VNE, "Vne"),
        ]
        .into_iter()
        .enumerate()
        {
            self.label(label, 280 + n as i32 * 82, 539, 73);
            self.edit(id, 280 + n as i32 * 82, 562, 72, 12);
        }
        self.label("Limits reference (manual / page / variant)", 280, 600, 400);
        self.edit(REFERENCE, 280, 623, 400, 512);
        self.label("VSI full scale (± ft/min)", 280, 666, 225);
        self.edit(VSI_MAX, 510, 662, 110, 5);
        self.label("Preview gauge", 726, 111, 452);
        self.combo(
            GAUGE,
            &[
                "Airspeed",
                "Attitude",
                "Altimeter",
                "Heading",
                "Turn coordinator",
                "Vertical speed",
                "Combined flight panel",
                "Telemetry debug",
            ],
            726,
            139,
            452,
        );
        self.control("STATIC", "", UNITS, 726, 180, 452, 42, 0);
        self.control("STATIC", "", LABEL_A, 726, 642, 220, 22, 0);
        self.edit(SAMPLE_A, 726, 666, 200, 20);
        self.control("STATIC", "", LABEL_B, 955, 642, 220, 22, 0);
        self.edit(SAMPLE_B, 955, 666, 200, 20);
        self.control("STATIC", "", STATUS, 22, 758, 1156, 34, 0);
        if let Err(e) = self.reload_catalog() {
            self.status(&e.to_string());
        }
        if let Some(p) = self.catalog.first().cloned() {
            self.load(p);
        } else {
            self.load_draft();
        }
        self.sample_fields();
        let _ = self.render_preview();
    }
    unsafe fn get(&self, id: u16) -> String {
        let hwnd = self.ctl(id);
        let len = GetWindowTextLengthW(hwnd).clamp(0, 4096);
        let mut text = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32);
        String::from_utf16_lossy(&text[..n.max(0) as usize])
    }
    unsafe fn set(&self, id: u16, text: &str) {
        SetWindowTextW(self.ctl(id), wide(text).as_ptr());
    }
    unsafe fn status(&self, text: &str) {
        self.set(STATUS, text);
    }
    unsafe fn title(&self) {
        SetWindowTextW(
            self.hwnd,
            wide(if self.dirty {
                "Aircraft profiles - Round Instruments *"
            } else {
                "Aircraft profiles - Round Instruments"
            })
            .as_ptr(),
        );
    }
    unsafe fn limits_enabled(&self) {
        let enabled = SendMessageW(self.ctl(LIMITS), BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
        for id in [VS0, VS1, VFE, VNO, VNE, REFERENCE] {
            EnableWindow(self.ctl(id), enabled as i32);
        }
    }
    unsafe fn load_draft(&mut self) {
        self.loading = true;
        self.set(NAME, &self.draft.name);
        self.set(PACKAGE, &self.draft.aircraft.package);
        self.set(VARIANT, &self.draft.aircraft.variant);
        SendMessageW(
            self.ctl(SIM),
            CB_SETCURSEL,
            match self.draft.aircraft.simulator {
                Simulator::Msfs2020 => 0,
                Simulator::Msfs2024 => 1,
                Simulator::Dcs => 2,
            },
            0,
        );
        self.set(
            ID,
            &format!(
                "{}  /  rev {}",
                self.draft.profile_uuid, self.draft.revision
            ),
        );
        self.set(
            AIR_MAX,
            &self.draft.gauges.airspeed.full_scale_kias.to_string(),
        );
        SendMessageW(
            self.ctl(SCALE),
            CB_SETCURSEL,
            usize::from(self.draft.gauges.airspeed.scale == AirspeedScale::Linear),
            0,
        );
        let limits = self.draft.gauges.airspeed.limits;
        SendMessageW(
            self.ctl(LIMITS),
            BM_SETCHECK,
            if limits.is_some() {
                BST_CHECKED
            } else {
                BST_UNCHECKED
            } as usize,
            0,
        );
        for (id, value) in [VS0, VS1, VFE, VNO, VNE].into_iter().zip(
            limits
                .map(|l| [l.vs0, l.vs1, l.vfe, l.vno, l.vne])
                .unwrap_or([0.; 5]),
        ) {
            self.set(
                id,
                &if limits.is_some() {
                    format!("{value}")
                } else {
                    String::new()
                },
            );
        }
        self.set(REFERENCE, &self.draft.gauges.airspeed.limits_reference);
        self.set(
            VSI_MAX,
            &self.draft.gauges.vertical_speed.full_scale_fpm.to_string(),
        );
        self.limits_enabled();
        self.loading = false;
        self.title();
    }
    unsafe fn load(&mut self, profile: Profile) {
        self.revision = Some(profile.revision);
        self.draft = profile;
        self.dirty = false;
        self.load_draft();
        self.select_current();
    }
    unsafe fn select_current(&self) {
        let index = self
            .catalog
            .iter()
            .position(|p| p.profile_uuid == self.draft.profile_uuid);
        SendMessageW(
            self.ctl(LIST),
            LB_SETCURSEL,
            index.map_or(usize::MAX, |n| n),
            0,
        );
    }
    unsafe fn reload_catalog(&mut self) -> io::Result<()> {
        let catalog = self.store.list()?;
        self.catalog = catalog.profiles;
        SendMessageW(self.ctl(LIST), LB_RESETCONTENT, 0, 0);
        for p in &self.catalog {
            SendMessageW(
                self.ctl(LIST),
                LB_ADDSTRING,
                0,
                wide(&p.name).as_ptr() as isize,
            );
        }
        if !catalog.issues.is_empty() {
            self.status(&format!(
                "{} invalid profile file(s) were kept and skipped. {}",
                catalog.issues.len(),
                catalog.issues[0]
            ));
        }
        Ok(())
    }
    unsafe fn number(&self, id: u16, name: &str) -> io::Result<f32> {
        self.get(id)
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| io::Error::other(format!("{name} must be a finite number.")))
    }
    unsafe fn form(&self) -> io::Result<Profile> {
        let mut p = self.draft.clone();
        p.name = self.get(NAME).trim().into();
        p.aircraft.package = self.get(PACKAGE).trim().into();
        p.aircraft.variant = self.get(VARIANT).trim().into();
        p.aircraft.simulator = match SendMessageW(self.ctl(SIM), CB_GETCURSEL, 0, 0) {
            0 => Simulator::Msfs2020,
            1 => Simulator::Msfs2024,
            2 => Simulator::Dcs,
            _ => return Err(io::Error::other("Choose a simulator.")),
        };
        p.gauges.airspeed.full_scale_kias = self
            .get(AIR_MAX)
            .trim()
            .parse()
            .map_err(|_| io::Error::other("Airspeed full scale must be an integer."))?;
        p.gauges.airspeed.scale = if SendMessageW(self.ctl(SCALE), CB_GETCURSEL, 0, 0) == 1 {
            AirspeedScale::Linear
        } else {
            AirspeedScale::Prototype
        };
        p.gauges.airspeed.limits =
            if SendMessageW(self.ctl(LIMITS), BM_GETCHECK, 0, 0) == BST_CHECKED as isize {
                Some(AirspeedLimits {
                    vs0: self.number(VS0, "Vs0")?,
                    vs1: self.number(VS1, "Vs1")?,
                    vfe: self.number(VFE, "Vfe")?,
                    vno: self.number(VNO, "Vno")?,
                    vne: self.number(VNE, "Vne")?,
                })
            } else {
                None
            };
        p.gauges.airspeed.limits_reference = self.get(REFERENCE).trim().into();
        p.gauges.vertical_speed.full_scale_fpm = self
            .get(VSI_MAX)
            .trim()
            .parse()
            .map_err(|_| io::Error::other("VSI full scale must be an integer."))?;
        Ok(p)
    }
    unsafe fn save(&mut self) -> io::Result<()> {
        let saved = self
            .store
            .save(&self.form()?, self.revision.map(|_| &self.draft))?;
        self.load(saved);
        self.reload_catalog()?;
        self.select_current();
        let saved = format!(
            "Saved '{}' — revision {}. Stored locally.",
            self.draft.name, self.draft.revision
        );
        match self.render_preview() {
            Ok(()) => self.status(&saved),
            Err(e) => self.status(&format!("{saved} Preview needs update: {e}")),
        }
        Ok(())
    }
    unsafe fn confirm_changes(&mut self) -> bool {
        if !self.dirty {
            return true;
        }
        match MessageBoxW(self.hwnd,wide("Save your changes before continuing?\n\nYes: Save profile. No: Discard edits. Cancel: Keep editing.").as_ptr(),wide("Unsaved aircraft profile").as_ptr(),MB_YESNOCANCEL|MB_ICONQUESTION){
            IDYES=>match self.save(){Ok(())=>true,Err(e)=>{self.status(&e.to_string());false}},IDNO=>true,_=>false,
        }
    }
    unsafe fn mark_preview_stale(&mut self) {
        self.preview_stale = true;
        self.set(UNITS, &format!("Preview needs update. {}", self.units));
    }
    unsafe fn sample_fields(&mut self) {
        let (first, second, units) = match self.gauge {
            Instrument::Airspeed => (
                "Indicated airspeed (KIAS)",
                "",
                "Knots indicated. Colored arcs appear only with verified limits.",
            ),
            Instrument::Attitude => (
                "Pitch (degrees)",
                "Roll (degrees)",
                "Positive pitch: nose up. Positive roll: right bank.",
            ),
            Instrument::Debug => (
                "Pitch (degrees)", "Roll (degrees)",
                "Telemetry debug layout. Desktop sample data only.",
            ),
            Instrument::Consolidated => (
                "Pitch (degrees)",
                "Roll (degrees)",
                "Combined firmware layout. Other readings use fixed preview samples.",
            ),
            Instrument::Altimeter => (
                "Pressure altitude (ft)",
                "Barometer (inHg)",
                "Feet / inHg. Preview uses the existing local barometric model.",
            ),
            Instrument::Heading => (
                "Magnetic heading (degrees)",
                "",
                "Magnetic reference. No true-heading substitution.",
            ),
            Instrument::Turn => (
                "Turn rate (degrees/s)",
                "Slip ball (−1 to +1)",
                "Standard rate: 3°/second. Turn rate and slip are independent.",
            ),
            Instrument::Vsi => (
                "Vertical speed (ft/min)",
                "",
                "Positive: climb. Negative: descent. Values beyond scale show overrange.",
            ),
        };
        self.set(LABEL_A, first);
        self.set(LABEL_B, second);
        self.units = units;
        self.mark_preview_stale();
        ShowWindow(
            self.ctl(SAMPLE_B),
            if second.is_empty() { SW_HIDE } else { SW_SHOW },
        );
        let sample = Sample::defaults(self.gauge);
        self.set(SAMPLE_A, &sample.first.to_string());
        self.set(SAMPLE_B, &sample.second.to_string());
    }
    unsafe fn render_preview(&mut self) -> io::Result<()> {
        self.mark_preview_stale();
        let p = self.form()?;
        let first = self.number(SAMPLE_A, "First preview value")?;
        let second = if matches!(
            self.gauge,
            Instrument::Altimeter | Instrument::Attitude | Instrument::Turn | Instrument::Consolidated | Instrument::Debug
        ) {
            self.number(SAMPLE_B, "Second preview value")?
        } else {
            0.
        };
        preview::render(
            &p.gauges,
            self.gauge,
            Sample { first, second },
            &mut self.pixels,
            480,
        )?;
        for (out, &pixel) in self.dib.iter_mut().zip(&self.pixels) {
            *out = ((((pixel >> 11) & 31) as u32 * 255 / 31) << 16)
                | ((((pixel >> 5) & 63) as u32 * 255 / 63) << 8)
                | ((pixel & 31) as u32 * 255 / 31);
        }
        self.preview_ready = true;
        self.preview_stale = false;
        self.set(UNITS, self.units);
        let rect = RECT {
            left: 726,
            top: 226,
            right: 1178,
            bottom: 638,
        };
        InvalidateRect(self.hwnd, &rect, 0);
        Ok(())
    }
    unsafe fn choose_file(&self, save: bool) -> io::Result<Option<PathBuf>> {
        let mut buffer = vec![0u16; 32768];
        if save {
            let suggested = wide(&format!("{}.json", self.draft.profile_uuid));
            buffer[..suggested.len()].copy_from_slice(&suggested);
        }
        let filter = wide("Aircraft profile JSON\0*.json\0\0");
        let title = wide(if save {
            "Export aircraft profile"
        } else {
            "Import aircraft profile as a new copy"
        });
        let ext = wide("json");
        let mut d: OPENFILENAMEW = std::mem::zeroed();
        d.lStructSize = size_of::<OPENFILENAMEW>() as u32;
        d.hwndOwner = self.hwnd;
        d.lpstrFilter = filter.as_ptr();
        d.lpstrFile = buffer.as_mut_ptr();
        d.nMaxFile = buffer.len() as u32;
        d.lpstrTitle = title.as_ptr();
        d.lpstrDefExt = ext.as_ptr();
        d.Flags = OFN_NOCHANGEDIR
            | OFN_PATHMUSTEXIST
            | if save {
                OFN_OVERWRITEPROMPT
            } else {
                OFN_FILEMUSTEXIST
            };
        if (if save {
            GetSaveFileNameW(&mut d)
        } else {
            GetOpenFileNameW(&mut d)
        }) == 0
        {
            let e = CommDlgExtendedError();
            return if e == 0 {
                Ok(None)
            } else {
                Err(io::Error::other(format!("File dialog failed: {e}")))
            };
        }
        let length = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        Ok(Some(PathBuf::from(String::from_utf16_lossy(
            &buffer[..length],
        ))))
    }
    unsafe fn command(&mut self, id: u16, notification: u16) -> io::Result<bool> {
        match id {
            SAVE if notification == BN_CLICKED as u16 => self.save()?,
            APPLY if notification == BN_CLICKED as u16 => {
                self.save()?;
                if self.draft.aircraft.simulator!=Simulator::Msfs2020 {return Err(io::Error::other("Live delivery currently supports MSFS 2020 profiles."));}
                self.store.set_active(&self.draft)?;
                self.status("Selected for live delivery. The tray app will reconnect; check Status for board confirmation. Select the matching aircraft manually.");
            }
            DEFAULTS if notification == BN_CLICKED as u16 => {
                self.store.clear_active()?;
                self.status("Default gauges selected. The tray app will reconnect and clear the live profile.");
            }
            PREVIEW if notification == BN_CLICKED as u16 => {
                self.render_preview()?;
                self.status(
                    "Preview updated from the current form. Save profile to keep these settings.",
                );
            }
            NEW if notification == BN_CLICKED as u16 => {
                if self.confirm_changes() {
                    self.draft = Profile::new(uuid()?);
                    self.revision = None;
                    self.dirty = false;
                    self.load_draft();
                    self.select_current();
                    self.render_preview()?;
                    self.status("New profile. Enter its exact aircraft package and variant, then Save profile.");
                }
            }
            DUPLICATE if notification == BN_CLICKED as u16 => {
                let mut p = self.form()?;
                p.profile_uuid = uuid()?.to_string();
                p.revision = 0;
                p.name = format!("{} (copy)", p.name.chars().take(93).collect::<String>());
                self.draft = p;
                self.revision = None;
                self.dirty = true;
                self.load_draft();
                self.select_current();
                self.status("Unsaved duplicate created. The original profile is unchanged.");
            }
            IMPORT if notification == BN_CLICKED as u16 => {
                if self.confirm_changes() {
                    if let Some(path) = self.choose_file(false)? {
                        let mut p = store::read_profile(&path)?;
                        p.profile_uuid = uuid()?.to_string();
                        p.revision = 0;
                        self.draft = p;
                        self.revision = None;
                        self.dirty = true;
                        self.load_draft();
                        self.select_current();
                        self.render_preview()?;
                        self.status("Imported as an unsaved copy with a new profile UUID. Review and Save profile.");
                    }
                }
            }
            EXPORT if notification == BN_CLICKED as u16 => {
                let p = self.form()?;
                p.validate()?;
                if let Some(path) = self.choose_file(true)? {
                    self.store.export(&p, &path)?;
                    self.status(&format!("Exported {}", path.display()));
                }
            }
            CLOSE if notification == BN_CLICKED as u16 => return Ok(self.confirm_changes()),
            LIST if notification == LBN_SELCHANGE as u16 => {
                let n = SendMessageW(self.ctl(LIST), LB_GETCURSEL, 0, 0);
                if let Some(profile) = self.catalog.get(n as usize).cloned() {
                    if profile.profile_uuid != self.draft.profile_uuid {
                        if self.confirm_changes() {
                            self.load(profile);
                            self.render_preview()?;
                            self.status("Profile loaded. Edit settings or choose a preview gauge.");
                        } else {
                            self.select_current();
                        }
                    }
                }
            }
            GAUGE if notification == CBN_SELCHANGE as u16 => {
                let n = SendMessageW(self.ctl(GAUGE), CB_GETCURSEL, 0, 0);
                self.gauge = *GAUGES.get(n as usize).unwrap_or(&Instrument::Airspeed);
                self.sample_fields();
                self.render_preview()?;
            }
            LIMITS if notification == BN_CLICKED as u16 => {
                self.limits_enabled();
                self.dirty = true;
                self.mark_preview_stale();
                self.title();
            }
            NAME | PACKAGE | VARIANT | AIR_MAX | VS0 | VS1 | VFE | VNO | VNE | REFERENCE
            | VSI_MAX
                if notification == EN_CHANGE as u16 =>
            {
                self.dirty = true;
                self.mark_preview_stale();
                self.title();
            }
            SIM | SCALE if notification == CBN_SELCHANGE as u16 => {
                self.dirty = true;
                self.mark_preview_stale();
                self.title();
            }
            SAMPLE_A | SAMPLE_B if notification == EN_CHANGE as u16 => self.mark_preview_stale(),
            _ => (),
        }
        Ok(false)
    }
    unsafe fn paint(&self) {
        let mut p: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(self.hwnd, &mut p);
        if self.preview_ready {
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = 480;
            info.bmiHeader.biHeight = -480;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            // Downscale the 480-pixel face without the default AND scan mode,
            // which erases thin bright ticks and glyph strokes on black.
            let saved_dc = SaveDC(dc);
            SetStretchBltMode(dc, HALFTONE);
            SetBrushOrgEx(dc, 0, 0, null_mut());
            StretchDIBits(
                dc,
                746,
                226,
                412,
                412,
                0,
                0,
                480,
                480,
                self.dib.as_ptr().cast(),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
            if saved_dc != 0 {
                RestoreDC(dc, saved_dc);
            }
        }
        EndPaint(self.hwnd, &p);
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        unsafe {
            if !self.font.is_null() {
                DeleteObject(self.font);
            }
            if !self.heading_font.is_null() {
                DeleteObject(self.heading_font);
            }
        }
    }
}

unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if message == WM_CTLCOLORSTATIC {
        SetBkMode(w as HDC, TRANSPARENT as i32);
        return GetSysColorBrush(COLOR_WINDOW) as isize;
    }
    // Each window owns exactly one boxed RefCell, released at WM_NCDESTROY.
    if message == WM_NCCREATE {
        let create = &*(l as *const CREATESTRUCTW);
        let transfer = &mut *(create.lpCreateParams as *mut Creation);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, transfer.pointer as isize);
        transfer.transferred = true;
        return 1;
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<Editor>;
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, w, l);
    }
    if message == WM_NCDESTROY {
        ACTIVE.with(|a| {
            if a.get() == hwnd {
                a.set(null_mut())
            }
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        let state = Box::from_raw(pointer);
        let standalone = state.borrow().standalone;
        drop(state);
        if standalone {
            PostQuitMessage(0);
        }
        return DefWindowProcW(hwnd, message, w, l);
    }
    let close = {
        let Ok(mut state) = (*pointer).try_borrow_mut() else {
            return DefWindowProcW(hwnd, message, w, l);
        };
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> io::Result<bool> {
                match message {
                    WM_CREATE => {
                        state.hwnd = hwnd;
                        state.setup();
                    }
                    WM_COMMAND if !state.loading => {
                        return state.command((w & 0xffff) as u16, ((w >> 16) & 0xffff) as u16)
                    }
                    WM_CLOSE => return Ok(state.confirm_changes()),
                    WM_PAINT => state.paint(),
                    _ => return Ok(false),
                }
                Ok(false)
            }));
        match result {
            Ok(Ok(close)) => close,
            Ok(Err(e)) => {
                state.status(&e.to_string());
                false
            }
            Err(_) => {
                state.status(
                    "The editor operation failed. Current files and your edits have been kept.",
                );
                false
            }
        }
    };
    // Release the RefCell borrow before synchronous destruction/reentrant messages.
    if close {
        DestroyWindow(hwnd);
        return 0;
    }
    if matches!(message, WM_CREATE | WM_COMMAND | WM_CLOSE | WM_PAINT) {
        return 0;
    }
    DefWindowProcW(hwnd, message, w, l)
}

pub fn open(root: PathBuf, owner: HWND, standalone: bool) -> io::Result<HWND> {
    // SAFETY: window class/static callback and window-owned state outlive all calls.
    unsafe {
        let editor = Editor::new(root, standalone)?;
        // Scope ownership by storage root so an isolated preview cannot reuse
        // a real profile window belonging to another data directory/process.
        let mut hash = 0xcbf29ce484222325u64;
        for b in editor
            .store
            .root()
            .to_string_lossy()
            .to_lowercase()
            .as_bytes()
        {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let name = wide(&format!("{CLASS}-{hash:016x}"));
        let existing = FindWindowW(name.as_ptr(), null());
        if !existing.is_null() {
            ShowWindow(existing, SW_RESTORE);
            SetForegroundWindow(existing);
            return Ok(existing);
        }
        let mut class: WNDCLASSW = std::mem::zeroed();
        class.lpfnWndProc = Some(procedure);
        class.hInstance = GetModuleHandleW(null());
        class.lpszClassName = name.as_ptr();
        class.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        class.hbrBackground = GetSysColorBrush(COLOR_WINDOW);
        if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(io::Error::last_os_error());
        }
        let state = Box::into_raw(Box::new(RefCell::new(editor)));
        let mut creation = Creation {
            pointer: state,
            transferred: false,
        };
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            name.as_ptr(),
            wide("Aircraft profiles - Round Instruments").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1220,
            850,
            owner,
            null_mut(),
            class.hInstance,
            (&mut creation as *mut Creation).cast(),
        );
        // WM_NCDESTROY reclaims state when creation progresses past WM_NCCREATE.
        if hwnd.is_null() {
            if !creation.transferred {
                drop(Box::from_raw(state));
            }
            return Err(io::Error::last_os_error());
        }
        ACTIVE.with(|a| a.set(hwnd));
        // Consume STARTF_USESHOWWINDOW from hidden launchers before showing
        // the explicitly requested editor, as the tray's hidden owner does.
        ShowWindow(hwnd, SW_HIDE);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        Ok(hwnd)
    }
}
pub fn standalone(root: Option<PathBuf>) -> io::Result<()> {
    let hwnd = open(root.map_or_else(default_root, Ok)?, null_mut(), true)?;
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid != std::process::id() {
        return Ok(());
    }
    unsafe {
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            if !route(&message) {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    Ok(())
}
pub fn route(message: &MSG) -> bool {
    ACTIVE.with(|a| unsafe {
        let hwnd = a.get();
        !hwnd.is_null()
            && GetAncestor(message.hwnd, GA_ROOT) == hwnd
            && IsDialogMessageW(hwnd, message) != 0
    })
}
#[cfg(test)]
mod tests {
    #[test]
    fn profile_ids_are_valid_and_fresh() {
        let a = super::uuid().unwrap();
        let b = super::uuid().unwrap();
        assert_ne!(a, b);
        assert_eq!(
            instrument_core::device::Uuid::parse(&a.to_string()),
            Some(a)
        );
    }
}
