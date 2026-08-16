use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
pub struct WorkerState {
    accepting_work: Arc<AtomicBool>,
}

impl Default for WorkerState {
    fn default() -> Self {
        Self {
            accepting_work: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl WorkerState {
    pub fn is_accepting_work(&self) -> bool {
        self.accepting_work.load(Ordering::SeqCst)
    }

    pub fn stop_claiming(&self) {
        self.accepting_work.store(false, Ordering::SeqCst);
    }
}
