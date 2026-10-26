//! Background device poller. Emits `device://status` only when the status changes.

use crate::device::probe;
use crate::events::Emitter;
use crate::ipc::DeviceStatus;
use crate::profile::DeviceProfile;
use crate::tools::Timeouts;
use crate::tools::adb::Adb;
use crate::tools::usb::UsbProbe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

type Wake = Arc<(Mutex<bool>, Condvar)>;

pub struct Poller {
    adb: Arc<dyn Adb>,
    usb: Arc<dyn UsbProbe>,
    profiles: Vec<DeviceProfile>,
    emitter: Arc<dyn Emitter>,
    paused: AtomicBool,
    stopped: AtomicBool,
    /// Held for the duration of a probe so `pause()` can wait for one in flight.
    poll_lock: Mutex<()>,
    wake: Wake,
    state: Mutex<(DeviceStatus, Option<String>)>,
}

impl Poller {
    /// Starts the polling thread. It waits `interval` of real time between polls (a condvar
    /// wait that `resume`/`stop`/drop wake early) and holds only a weak reference, so dropping
    /// the last `Arc` (or calling `stop`) ends it.
    pub fn spawn(
        adb: Arc<dyn Adb>,
        usb: Arc<dyn UsbProbe>,
        profiles: Vec<DeviceProfile>,
        emitter: Arc<dyn Emitter>,
        interval: Duration,
    ) -> Arc<Poller> {
        let wake: Wake = Arc::new((Mutex::new(false), Condvar::new()));
        let p = Arc::new(Poller {
            adb,
            usb,
            profiles,
            emitter,
            paused: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            poll_lock: Mutex::new(()),
            wake: wake.clone(),
            state: Mutex::new((DeviceStatus::None, None)),
        });
        let weak = Arc::downgrade(&p);
        std::thread::spawn(move || {
            loop {
                // Wait first: the owner calls `poll_once` for the initial status, and
                // `resume`/`stop` wake this early.
                let (m, cv) = &*wake;
                let g = m.lock().unwrap_or_else(|e| e.into_inner());
                let (mut g, _) = cv
                    .wait_timeout_while(g, interval, |woken| !*woken)
                    .unwrap_or_else(|e| e.into_inner());
                *g = false;
                drop(g);
                let Some(p) = weak.upgrade() else { break };
                if p.stopped.load(Ordering::SeqCst) {
                    break;
                }
                p.poll_once();
            }
        });
        p
    }

    fn state(&self) -> MutexGuard<'_, (DeviceStatus, Option<String>)> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn nudge(&self) {
        let (m, cv) = &*self.wake;
        *m.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cv.notify_all();
    }

    /// One probe cycle; skipped while paused or stopped.
    pub fn poll_once(&self) {
        let _guard = self.poll_lock.lock().unwrap_or_else(|e| e.into_inner());
        if self.paused.load(Ordering::SeqCst) || self.stopped.load(Ordering::SeqCst) {
            return;
        }
        let (status, serial) = probe(
            self.adb.as_ref(),
            self.usb.as_ref(),
            &self.profiles,
            &Timeouts::default(),
        );
        let changed = {
            let mut s = self.state();
            let changed = s.0 != status;
            *s = (status.clone(), serial);
            changed
        };
        if changed {
            self.emitter.device(&status);
        }
    }

    /// Returns only once no probe is running; no adb/usb call or event follows.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        drop(self.poll_lock.lock().unwrap_or_else(|e| e.into_inner()));
    }
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.nudge();
    }
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.nudge();
    }
    pub fn latest(&self) -> DeviceStatus {
        self.state().0.clone()
    }
    pub fn serial(&self) -> Option<String> {
        self.state().1.clone()
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.nudge();
    }
}
