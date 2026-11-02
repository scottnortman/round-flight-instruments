//! Bounded asynchronous wire logs. Saturation/errors stop the session visibly.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};
const PART_BYTES: usize = 4 * 1024 * 1024;
const PARTS: usize = 8;
pub struct Log {
    tx: Option<SyncSender<String>>,
    worker: Option<JoinHandle<()>>,
    failure: Arc<Mutex<Option<String>>>,
    pub directory: PathBuf,
}
fn file(directory: &Path, index: usize) -> io::Result<File> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(directory.join(format!("events-{index:02}.jsonl")))
}
impl Log {
    pub fn open(root: &Path, session: u64) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let directory = root.join(format!("session-{session}"));
        fs::create_dir(&directory)?; // never reuse/overwrite prior evidence
        let mut output = file(&directory, 0)?;
        let worker_dir = directory.clone();
        let failure = Arc::new(Mutex::new(None));
        let worker_failure = failure.clone();
        let (tx, rx) = mpsc::sync_channel::<String>(128);
        let worker = thread::spawn(move || {
            let result = (|| -> io::Result<()> {
                let (mut bytes, mut index) = (0, 0);
                for line in rx {
                    if bytes + line.len() + 1 > PART_BYTES {
                        index = (index + 1) % PARTS;
                        let path = worker_dir.join(format!("events-{index:02}.jsonl"));
                        // Only these known files in this newly created session rotate.
                        if path.exists() {
                            fs::remove_file(path)?;
                        }
                        output = file(&worker_dir, index)?;
                        bytes = 0;
                    }
                    writeln!(output, "{line}")?;
                    bytes += line.len() + 1;
                }
                output.flush()
            })();
            if let Err(e) = result {
                *worker_failure.lock().unwrap() = Some(e.to_string());
            }
        });
        Ok(Self {
            tx: Some(tx),
            worker: Some(worker),
            failure,
            directory,
        })
    }
    pub fn check(&self) -> io::Result<()> {
        if let Some(message) = self.failure.lock().unwrap().as_ref() {
            return Err(io::Error::other(message.clone()));
        }
        Ok(())
    }
    pub fn record(&self, value: serde_json::Value) -> io::Result<()> {
        self.check()?;
        let line = value.to_string();
        if line.len() > 8192 {
            return Err(io::Error::other("diagnostic record too large"));
        }
        self.tx
            .as_ref()
            .ok_or_else(|| io::Error::other("log closed"))?
            .try_send(line)
            .map_err(|_| io::Error::other("diagnostic queue full or writer stopped"))
    }
    pub fn finish(&mut self) -> io::Result<()> {
        self.tx.take();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("log writer panicked"))?;
        }
        self.check()
    }
}
impl Drop for Log {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
