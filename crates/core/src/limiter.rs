//! Limiter di picco con lookahead: interviene solo sui picchi oltre soglia,
//! senza comprimere la dinamica del resto del segnale.
//!
//! Il guadagno richiesto da ogni campione viene trattenuto sul minimo della
//! finestra futura (`la` campioni) e poi mediato su `la` campioni: la media di
//! minimi che includono tutti il campione di picco garantisce che il picco non
//! superi mai la soglia, senza gradini di guadagno udibili.

use std::collections::VecDeque;

pub struct Limiter {
    la: usize,
    threshold: f32,
    release: f32,
    idx: usize,
    minq: VecDeque<(usize, f32)>,
    xdelay: VecDeque<f32>,
    boxq: VecDeque<f32>,
    box_sum: f64,
    hold: f32,
    pushed: usize,
    emitted: usize,
}

impl Limiter {
    /// `threshold_db` (es. −1 dBFS), lookahead e rilascio in millisecondi.
    pub fn new(rate: u32, threshold_db: f32, lookahead_ms: f32, release_ms: f32) -> Self {
        let la = ((rate as f32 * lookahead_ms / 1000.0) as usize).max(1);
        let release = 1.0 - (-1.0 / (rate as f32 * release_ms / 1000.0)).exp();
        Self {
            la,
            threshold: 10f32.powf(threshold_db / 20.0),
            release,
            idx: 0,
            minq: VecDeque::new(),
            xdelay: VecDeque::new(),
            boxq: VecDeque::new(),
            box_sum: 0.0,
            hold: 1.0,
            pushed: 0,
            emitted: 0,
        }
    }

    fn step(&mut self, x: f32, out: &mut Vec<f32>) {
        let a = x.abs();
        let g = if a > self.threshold { self.threshold / a } else { 1.0 };
        while self.minq.back().is_some_and(|b| b.1 >= g) {
            self.minq.pop_back();
        }
        self.minq.push_back((self.idx, g));
        self.xdelay.push_back(x);
        if self.idx + 1 >= self.la {
            let k = self.idx + 1 - self.la;
            while self.minq.front().is_some_and(|f| f.0 < k) {
                self.minq.pop_front();
            }
            let mut h = self.minq.front().map(|f| f.1).unwrap_or(1.0);
            h = h.min(self.hold + (1.0 - self.hold) * self.release);
            self.hold = h;
            self.boxq.push_back(h);
            self.box_sum += h as f64;
            if self.boxq.len() > self.la {
                self.box_sum -= self.boxq.pop_front().unwrap() as f64;
            }
            let gain = (self.box_sum / self.boxq.len() as f64) as f32;
            let y = self.xdelay.pop_front().unwrap() * gain;
            out.push(y.clamp(-1.0, 1.0));
            self.emitted += 1;
        }
        self.idx += 1;
    }

    /// Elabora `input`, accodando l'uscita (ritardata di `la − 1` campioni) a `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for &x in input {
            self.pushed += 1;
            self.step(x, out);
        }
    }

    /// Emette i campioni ancora in coda.
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        while self.emitted < self.pushed {
            self.step(0.0, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_are_limited_and_quiet_parts_untouched() {
        let mut lim = Limiter::new(48_000, -1.0, 5.0, 100.0);
        let mut input = vec![0.2f32; 48_000];
        input[24_000] = 1.8;
        input[24_001] = -1.5;
        let mut out = Vec::new();
        lim.process(&input, &mut out);
        lim.flush(&mut out);
        assert_eq!(out.len(), input.len());
        let thr = 10f32.powf(-1.0 / 20.0);
        assert!(out.iter().all(|v| v.abs() <= thr + 1e-4));
        assert!((out[1000] - 0.2).abs() < 1e-6);
        assert!((out[47_000] - 0.2).abs() < 1e-3);
    }
}
