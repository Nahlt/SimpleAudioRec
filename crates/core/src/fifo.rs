//! Coda di una sorgente audio: riceve campioni mono alla frequenza nativa del
//! dispositivo, li porta a 48 kHz (con la correzione di deriva corrente) e li
//! rende disponibili al mixer.

use crate::resampler::Resampler;
use crate::{SAMPLE_RATE, lock};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const MAX_BUFFERED: usize = SAMPLE_RATE as usize * 5;

struct Inner {
    rs: Option<(u32, Resampler)>,
    buf: VecDeque<f32>,
    tmp: Vec<f32>,
}

pub struct SourceFifo {
    inner: Mutex<Inner>,
    rel_bits: AtomicU64,
}

impl SourceFifo {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                rs: None,
                buf: VecDeque::with_capacity(SAMPLE_RATE as usize),
                tmp: Vec::with_capacity(4096),
            }),
            rel_bits: AtomicU64::new(1.0f64.to_bits()),
        })
    }

    /// Accoda campioni mono alla frequenza `rate` (chiamato dal thread di cattura).
    pub fn push(&self, mono: &[f32], rate: u32) {
        let rel = f64::from_bits(self.rel_bits.load(Ordering::Relaxed));
        let mut g = lock(&self.inner);
        let inner = &mut *g;
        if inner.rs.as_ref().map(|(r, _)| *r) != Some(rate) {
            inner.rs = Some((rate, Resampler::new(rate, SAMPLE_RATE)));
        }
        let (_, rs) = inner.rs.as_mut().expect("resampler");
        rs.set_rel(rel);
        inner.tmp.clear();
        rs.process(mono, &mut inner.tmp);
        inner.buf.extend(inner.tmp.iter().copied());
        if inner.buf.len() > MAX_BUFFERED {
            let excess = inner.buf.len() - MAX_BUFFERED;
            inner.buf.drain(..excess);
        }
    }

    /// Campioni a 48 kHz disponibili.
    pub fn len(&self) -> usize {
        lock(&self.inner).buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Estrae fino a `out.len()` campioni; il resto di `out` è riempito di zeri.
    /// Restituisce quanti campioni reali sono stati estratti.
    pub fn pull(&self, out: &mut [f32]) -> usize {
        let mut g = lock(&self.inner);
        let n = out.len().min(g.buf.len());
        for (o, v) in out.iter_mut().zip(g.buf.drain(..n)) {
            *o = v;
        }
        out[n..].fill(0.0);
        n
    }

    /// Scarta i `n` campioni più vecchi.
    pub fn discard(&self, n: usize) {
        let mut g = lock(&self.inner);
        let n = n.min(g.buf.len());
        g.buf.drain(..n);
    }

    /// Imposta la correzione di deriva (vedi [`Resampler::set_rel`]).
    pub fn set_rel(&self, rel: f64) {
        self.rel_bits.store(rel.to_bits(), Ordering::Relaxed);
    }
}
