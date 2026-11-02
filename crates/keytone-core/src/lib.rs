//! Keytone core: everything except the UI.
//!
//! ```text
//! keyhook ──KeyEvent──▶ dispatch thread ──▶ Mapper ──PlayCmd──▶ Engine (cpal)
//!                                  └──▶ observer (UI highlight)
//! ```

pub mod engine;
pub mod library;
pub mod mapper;
pub mod mechvibes;
pub mod pack;
pub mod recorder;
pub mod sound;

pub use keyhook;

use engine::Engine;
use keyhook::KeyEvent;
use mapper::Mapper;
use pack::LoadedPack;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

type Observer = Box<dyn Fn(KeyEvent) + Send + Sync>;

/// Shared playback state. Cheap to clone (it's an `Arc` inside).
#[derive(Clone)]
pub struct Player {
    inner: Arc<PlayerInner>,
}

struct PlayerInner {
    engine: Engine,
    pack: RwLock<Option<Arc<LoadedPack>>>,
    mapper: Mutex<Mapper>,
    enabled: AtomicBool,
    observer: RwLock<Option<Observer>>,
}

impl Player {
    pub fn new(engine: Engine) -> Player {
        Player {
            inner: Arc::new(PlayerInner {
                engine,
                pack: RwLock::new(None),
                mapper: Mutex::new(Mapper::new()),
                enabled: AtomicBool::new(true),
                observer: RwLock::new(None),
            }),
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.inner.engine
    }

    pub fn set_pack(&self, pack: Option<Arc<LoadedPack>>) {
        *self.inner.pack.write() = pack;
    }

    pub fn pack(&self) -> Option<Arc<LoadedPack>> {
        self.inner.pack.read().clone()
    }

    pub fn set_enabled(&self, on: bool) {
        self.inner.enabled.store(on, Ordering::Relaxed);
        if !on {
            self.inner.mapper.lock().reset();
        }
    }

    pub fn enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Relaxed)
    }

    /// Called for every key event (whether or not a sound plays), e.g. to
    /// light up keys in the UI.
    pub fn set_observer(&self, f: impl Fn(KeyEvent) + Send + Sync + 'static) {
        *self.inner.observer.write() = Some(Box::new(f));
    }

    /// Handle one key event. Fast; safe to call from the dispatch thread.
    pub fn handle(&self, ev: KeyEvent) {
        if let Some(obs) = self.inner.observer.read().as_ref() {
            obs(ev);
        }
        if !self.enabled() {
            return;
        }
        let pack = self.inner.pack.read().clone();
        if let Some(pack) = pack {
            if let Some(cmd) = self.inner.mapper.lock().handle(ev, &pack) {
                self.inner.engine.play(cmd);
            }
        }
    }

    /// Start the global key hook and a dispatch thread feeding this player.
    pub fn start_hook(&self) -> Result<(), keyhook::HookError> {
        let (tx, rx) = crossbeam_channel::bounded::<KeyEvent>(1024);
        keyhook::start(move |ev| {
            let _ = tx.try_send(ev);
        })?;
        let me = self.clone();
        std::thread::Builder::new()
            .name("keytone-dispatch".into())
            .spawn(move || {
                for ev in rx {
                    me.handle(ev);
                }
            })
            .map_err(|e| keyhook::HookError::Other(e.to_string()))?;
        Ok(())
    }
}
