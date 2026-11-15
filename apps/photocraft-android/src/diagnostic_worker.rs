//! A bounded serial worker for periodic diagnostic saves, testable on a host.
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};

pub struct Snapshot { pub generation: u64 }

/// Check while holding the same storage lock as debug toggles and explicit saves.
pub fn snapshot_is_current(snapshot: &Snapshot, generation: u64, enabled: bool) -> bool {
    enabled && snapshot.generation == generation
}

enum Message { Snapshot(Snapshot), Stop }

pub struct Worker {
    sender: SyncSender<Message>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn new(mut write: impl FnMut(Snapshot) + Send + 'static) -> Result<Self, std::io::Error> {
        // At most one request runs and one waits; on a full queue the UI retries the dirty log.
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new().name("photodroid-log".into()).spawn(move || {
            while let Ok(message) = receiver.recv() {
                match message {
                    Message::Snapshot(snapshot) => write(snapshot),
                    Message::Stop => break,
                }
            }
        })?;
        Ok(Self { sender, thread: Some(thread) })
    }

    pub fn enqueue(&self, generation: u64) -> Result<(), &'static str> {
        self.sender.try_send(Message::Snapshot(Snapshot { generation })).map_err(|e| match e {
            TrySendError::Full(_) => "diagnostic writer is busy",
            TrySendError::Disconnected(_) => "diagnostic writer stopped",
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Shutdown may wait for I/O, but never during a normal UI frame.
        let _ = self.sender.send(Message::Stop);
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, mpsc};
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn disabled_or_superseded_snapshots_are_not_written() {
        let job = Snapshot { generation: 4 };
        assert!(snapshot_is_current(&job, 4, true));
        assert!(!snapshot_is_current(&job, 4, false));
        assert!(!snapshot_is_current(&job, 5, true));
    }

    #[test]
    fn old_queued_generation_does_not_recreate_deleted_logs() {
        let generation = Arc::new(AtomicU64::new(1));
        let storage = Arc::new(Mutex::new(Vec::new()));
        let (started_tx, started_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (epoch, output) = (generation.clone(), storage.clone());
        let worker = Worker::new(move |snapshot| {
            started_tx.send(()).unwrap();
            resume_rx.recv().unwrap();
            let mut saved = output.lock().unwrap();
            if snapshot_is_current(&snapshot, epoch.load(Ordering::SeqCst), true) {
                saved.push(snapshot.generation);
            }
        }).unwrap();
        worker.enqueue(1).unwrap();
        started_rx.recv().unwrap();
        generation.fetch_add(1, Ordering::SeqCst);
        storage.lock().unwrap().clear();
        resume_tx.send(()).unwrap();
        drop(worker);
        assert!(storage.lock().unwrap().is_empty());
    }

    #[test]
    fn serial_bounded_and_drained_at_shutdown() {
        let (started_tx, started_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let written = Arc::new(Mutex::new(Vec::new()));
        let out = written.clone();
        let worker = Worker::new(move |snapshot| {
            if snapshot.generation == 1 {
                started_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            }
            out.lock().unwrap().push(snapshot.generation);
        }).unwrap();
        worker.enqueue(1).unwrap();
        started_rx.recv().unwrap();
        worker.enqueue(2).unwrap();
        assert_eq!(worker.enqueue(3), Err("diagnostic writer is busy"));
        resume_tx.send(()).unwrap();
        drop(worker);
        assert_eq!(*written.lock().unwrap(), vec![1, 2]);
    }
}
