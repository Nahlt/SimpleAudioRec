//! Mixer a due canali con allineamento temporale.
//!
//! L'orologio di riferimento è quello del sistema: a ogni passo il mixer estrae
//! da ciascuna sorgente esattamente i campioni corrispondenti al tempo trascorso.
//! Ogni sorgente è tenuta a un livello di riempimento costante
//! ([`TARGET_LATENCY`]) ritoccando di poco il rapporto di ricampionamento: così
//! la deriva tra gli orologi dei dispositivi viene assorbita e i canali restano
//! allineati. Se una sorgente si interrompe (dispositivo scollegato, loopback
//! senza audio) il canale va in silenzio e, quando i dati riprendono, si
//! ricostituisce la stessa latenza prima di riprendere a leggere.

use crate::fifo::SourceFifo;
use std::sync::Arc;

/// Livello di riempimento obiettivo di ogni sorgente (100 ms a 48 kHz).
pub const TARGET_LATENCY: usize = 4_800;
const KP: f64 = 4e-6;
const EMA_ALPHA: f64 = 0.02;
const MAX_CORRECTION: f64 = 0.003;

#[derive(Default)]
struct SourceState {
    buffering: bool,
    ema: f64,
}

impl SourceState {
    fn new() -> Self {
        Self { buffering: true, ema: 0.0 }
    }
}

fn pull_source(fifo: &SourceFifo, st: &mut SourceState, buf: &mut [f32]) {
    let level = fifo.len();
    if level > TARGET_LATENCY * 4 {
        // Il mixer è rimasto indietro (es. sistema sotto carico): riallinea.
        fifo.discard(level - TARGET_LATENCY);
        st.ema = 0.0;
    }
    if st.buffering {
        if fifo.len() >= TARGET_LATENCY {
            st.buffering = false;
            st.ema = 0.0;
        } else {
            buf.fill(0.0);
            return;
        }
    }
    let err = fifo.len() as f64 - TARGET_LATENCY as f64;
    st.ema += (err - st.ema) * EMA_ALPHA;
    let rel = (1.0 - st.ema * KP).clamp(1.0 - MAX_CORRECTION, 1.0 + MAX_CORRECTION);
    fifo.set_rel(rel);
    if fifo.pull(buf) < buf.len() {
        // Sorgente interrotta: silenzio, poi si ricostituisce la latenza.
        st.buffering = true;
        fifo.set_rel(1.0);
    }
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// Picchi del passo di mixing appena eseguito.
#[derive(Clone, Copy, Debug)]
pub struct TickPeaks {
    pub mic: f32,
    pub out: f32,
}

pub struct MixerCore {
    mic: Arc<SourceFifo>,
    out: Arc<SourceFifo>,
    mic_st: SourceState,
    out_st: SourceState,
    mic_buf: Vec<f32>,
    out_buf: Vec<f32>,
}

impl MixerCore {
    pub fn new(mic: Arc<SourceFifo>, out: Arc<SourceFifo>) -> Self {
        Self {
            mic,
            out,
            mic_st: SourceState::new(),
            out_st: SourceState::new(),
            mic_buf: Vec::new(),
            out_buf: Vec::new(),
        }
    }

    /// Estrae `n` frame da ciascuna sorgente.
    pub fn tick(&mut self, n: usize) -> TickPeaks {
        self.mic_buf.resize(n, 0.0);
        self.out_buf.resize(n, 0.0);
        pull_source(&self.mic, &mut self.mic_st, &mut self.mic_buf);
        pull_source(&self.out, &mut self.out_st, &mut self.out_buf);
        TickPeaks { mic: peak(&self.mic_buf), out: peak(&self.out_buf) }
    }

    /// Scrive in `dst` i frame dell'ultimo passo, interleaved: sinistro = microfono,
    /// destro = output. Con `mic_muted` il canale del microfono è silenzio.
    pub fn interleave_i16(&self, mic_muted: bool, dst: &mut Vec<i16>) {
        dst.clear();
        dst.reserve(self.mic_buf.len() * 2);
        for (m, o) in self.mic_buf.iter().zip(&self.out_buf) {
            let m = if mic_muted { 0.0 } else { *m };
            dst.push(to_i16(m));
            dst.push(to_i16(*o));
        }
    }
}

pub fn to_i16(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SAMPLE_RATE;

    /// Sorgente simulata: produce campioni alla propria frequenza dichiarata, ma con
    /// un orologio sbagliato di `drift` (es. 0.002 = 2000 ppm). Emette un impulso
    /// negli istanti (tempo reale simulato) indicati.
    struct SimSource {
        rate: u32,
        drift: f64,
        produced: u64,
        clicks: Vec<f64>,
    }

    impl SimSource {
        fn step(&mut self, t_end: f64, fifo: &SourceFifo) {
            let target = (t_end * self.rate as f64 * (1.0 + self.drift)) as u64;
            let n = (target - self.produced) as usize;
            let mut chunk = vec![0f32; n];
            for &c in &self.clicks {
                let idx = (c * self.rate as f64 * (1.0 + self.drift)) as u64;
                if idx >= self.produced && idx < target {
                    chunk[(idx - self.produced) as usize] = 1.0;
                }
            }
            self.produced = target;
            fifo.push(&chunk, self.rate);
        }
    }

    fn find_clicks(x: &[i16]) -> Vec<usize> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < x.len() {
            if x[i] > 8000 {
                // massimo locale del lobo principale
                let end = (i + 64).min(x.len());
                let (j, _) = x[i..end].iter().enumerate().max_by_key(|(_, v)| **v).unwrap();
                out.push(i + j);
                i += 4800;
            } else {
                i += 1;
            }
        }
        out
    }

    #[test]
    fn channels_stay_aligned_with_clock_drift() {
        let mic = SourceFifo::new();
        let out = SourceFifo::new();
        let duration = 150.0;
        let clicks = vec![2.0, duration - 3.0];
        let mut a = SimSource { rate: 48_000, drift: 0.002, produced: 0, clicks: clicks.clone() };
        let mut b = SimSource { rate: 44_100, drift: -0.0015, produced: 0, clicks };
        let mut mixer = MixerCore::new(mic.clone(), out.clone());
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut inter = Vec::new();
        let tick = SAMPLE_RATE as usize / 100;
        let steps = (duration * 100.0) as usize;
        for s in 1..=steps {
            let t = s as f64 / 100.0;
            a.step(t, &mic);
            b.step(t, &out);
            mixer.tick(tick);
            mixer.interleave_i16(false, &mut inter);
            for f in inter.chunks(2) {
                left.push(f[0]);
                right.push(f[1]);
            }
        }
        let cl = find_clicks(&left);
        let cr = find_clicks(&right);
        assert_eq!(cl.len(), 2, "{cl:?}");
        assert_eq!(cr.len(), 2, "{cr:?}");
        for k in 0..2 {
            let diff = (cl[k] as i64 - cr[k] as i64).abs();
            assert!(diff < 48 * 25, "click {k}: sinistro {} destro {} (diff {diff})", cl[k], cr[k]);
            // Posizione assoluta: istante reale + latenza (~100 ms), con tolleranza.
            let expected = [2.0, duration - 3.0][k] * 48_000.0 + TARGET_LATENCY as f64;
            assert!((cl[k] as f64 - expected).abs() < 48.0 * 40.0, "posizione {} attesa {expected}", cl[k]);
        }
    }

    #[test]
    fn silent_gap_resumes_with_same_latency() {
        let mic = SourceFifo::new();
        let out = SourceFifo::new();
        let mut mixer = MixerCore::new(mic.clone(), out.clone());
        let tick = 480;
        let mut right = Vec::new();
        let mut inter = Vec::new();
        for s in 0..600 {
            // il microfono produce sempre; l'output si interrompe tra 2 e 4 s
            mic.push(&vec![0.0; tick], 48_000);
            let t = s as f64 / 100.0;
            if !(2.0..4.0).contains(&t) {
                let mut chunk = vec![0.0f32; tick];
                if s == 500 {
                    chunk[0] = 1.0;
                }
                out.push(&chunk, 48_000);
            }
            mixer.tick(tick);
            mixer.interleave_i16(false, &mut inter);
            right.extend(inter.chunks(2).map(|f| f[1]));
        }
        let c = find_clicks(&right);
        assert_eq!(c.len(), 1);
        let expected = 500 * tick + TARGET_LATENCY;
        assert!((c[0] as i64 - expected as i64).abs() < 600, "{} vs {expected}", c[0]);
    }
}
