//! Ejecución de tareas de red en hilos de fondo. Los resultados vuelven a la interfaz
//! como mensajes que se procesan al inicio de cada frame.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

pub struct Outbox<M> {
    tx: Sender<M>,
    ctx: egui::Context,
}

impl<M> Outbox<M> {
    pub fn send(&self, msg: M) {
        let _ = self.tx.send(msg);
        self.ctx.request_repaint();
    }
}

pub struct Worker<M> {
    tx: Sender<M>,
    rx: Receiver<M>,
    ctx: egui::Context,
    running: Arc<AtomicUsize>,
}

impl<M: Send + 'static> Worker<M> {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, ctx, running: Arc::new(AtomicUsize::new(0)) }
    }

    /// Ejecuta `job` en un hilo nuevo; puede enviar tantos mensajes como quiera.
    pub fn run(&self, job: impl FnOnce(&Outbox<M>) + Send + 'static) {
        let out = Outbox { tx: self.tx.clone(), ctx: self.ctx.clone() };
        let running = self.running.clone();
        running.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            job(&out);
            running.fetch_sub(1, Ordering::SeqCst);
            out.ctx.request_repaint();
        });
    }

    /// Atajo para tareas que producen un único mensaje.
    pub fn spawn(&self, job: impl FnOnce() -> M + Send + 'static) {
        self.run(move |out| out.send(job()));
    }

    pub fn poll(&self) -> Vec<M> {
        self.rx.try_iter().collect()
    }

    pub fn busy(&self) -> bool {
        self.running.load(Ordering::SeqCst) > 0
    }
}
