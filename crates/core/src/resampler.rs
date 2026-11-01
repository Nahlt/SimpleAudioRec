//! Ricampionatore mono a rapporto variabile (sinc con finestra di Kaiser, polifase).
//!
//! Serve sia per convertire la frequenza nativa di un dispositivo a 48 kHz, sia
//! per compensare la deriva tra gli orologi dei dispositivi: il rapporto può
//! essere ritoccato di continuo con [`Resampler::set_rel`] senza discontinuità.

use std::f64::consts::PI;

const HALF_TAPS: usize = 16;
const TAPS: usize = 2 * HALF_TAPS;
const PHASES: usize = 256;
const KAISER_BETA: f64 = 8.0;

pub struct Resampler {
    base_ratio: f64,
    step: f64,
    table: Vec<f32>,
    hist: Vec<f32>,
    pos: f64,
}

fn bessel_i0(x: f64) -> f64 {
    let y = x * x / 4.0;
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..64 {
        term *= y / ((k * k) as f64);
        sum += term;
        if term < sum * 1e-14 {
            break;
        }
    }
    sum
}

impl Resampler {
    /// Crea un ricampionatore da `in_rate` a `out_rate` Hz.
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let base_ratio = out_rate as f64 / in_rate as f64;
        // Frequenza di taglio relativa alla Nyquist d'ingresso (anti-aliasing in discesa).
        let cutoff = base_ratio.min(1.0) * 0.94;
        let i0b = bessel_i0(KAISER_BETA);
        let mut table = vec![0f32; (PHASES + 1) * TAPS];
        for p in 0..=PHASES {
            let frac = p as f64 / PHASES as f64;
            let mut row = [0f64; TAPS];
            let mut sum = 0.0;
            for (k, r) in row.iter_mut().enumerate() {
                let d = frac + (HALF_TAPS as f64 - 1.0) - k as f64;
                let x = d / HALF_TAPS as f64;
                let w = if x.abs() >= 1.0 {
                    0.0
                } else {
                    bessel_i0(KAISER_BETA * (1.0 - x * x).sqrt()) / i0b
                };
                let arg = PI * cutoff * d;
                let s = if arg.abs() < 1e-12 { 1.0 } else { arg.sin() / arg };
                *r = cutoff * s * w;
                sum += *r;
            }
            for k in 0..TAPS {
                table[p * TAPS + k] = (row[k] / sum) as f32;
            }
        }
        Self {
            base_ratio,
            step: 1.0 / base_ratio,
            table,
            hist: vec![0.0; HALF_TAPS],
            pos: (HALF_TAPS - 1) as f64,
        }
    }

    /// Imposta la correzione relativa del rapporto (1.0 = nessuna correzione).
    /// Valori < 1 producono meno campioni in uscita.
    pub fn set_rel(&mut self, rel: f64) {
        self.step = 1.0 / (self.base_ratio * rel);
    }

    /// Elabora `input` aggiungendo i campioni prodotti in coda a `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.hist.extend_from_slice(input);
        loop {
            let i0 = self.pos as usize;
            if i0 + HALF_TAPS >= self.hist.len() {
                break;
            }
            let frac = self.pos - i0 as f64;
            let pf = frac * PHASES as f64;
            let p = (pf as usize).min(PHASES - 1);
            let a = (pf - p as f64) as f32;
            let r0 = &self.table[p * TAPS..p * TAPS + TAPS];
            let r1 = &self.table[(p + 1) * TAPS..(p + 1) * TAPS + TAPS];
            let base = i0 + 1 - HALF_TAPS;
            let x = &self.hist[base..base + TAPS];
            let mut acc = 0f32;
            for k in 0..TAPS {
                acc += (r0[k] + (r1[k] - r0[k]) * a) * x[k];
            }
            out.push(acc);
            self.pos += self.step;
        }
        let keep_from = (self.pos as usize + 1).saturating_sub(HALF_TAPS);
        if keep_from > 0 {
            self.hist.drain(..keep_from);
            self.pos -= keep_from as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f64, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f64 / rate as f64).sin() as f32 * 0.5)
            .collect()
    }

    /// Ampiezza della componente a `freq` (correlazione con seno e coseno).
    fn amplitude(x: &[f32], rate: u32, freq: f64) -> f64 {
        let (mut s, mut c) = (0.0, 0.0);
        for (i, v) in x.iter().enumerate() {
            let ph = 2.0 * PI * freq * i as f64 / rate as f64;
            s += *v as f64 * ph.sin();
            c += *v as f64 * ph.cos();
        }
        2.0 * (s * s + c * c).sqrt() / x.len() as f64
    }

    #[test]
    fn upsample_preserves_tone_and_length() {
        let input = sine(44_100, 1000.0, 44_100);
        let mut rs = Resampler::new(44_100, 48_000);
        let mut out = Vec::new();
        for chunk in input.chunks(441) {
            rs.process(chunk, &mut out);
        }
        assert!((out.len() as i64 - 48_000).abs() < 40, "len {}", out.len());
        let a = amplitude(&out[1000..47_000], 48_000, 1000.0);
        assert!((a - 0.5).abs() < 0.01, "amp {a}");
    }

    #[test]
    fn downsample_rejects_alias() {
        // 30 kHz a 96 kHz: oltre la Nyquist di 48 kHz, deve sparire.
        let input = sine(96_000, 30_000.0, 96_000);
        let mut rs = Resampler::new(96_000, 48_000);
        let mut out = Vec::new();
        rs.process(&input, &mut out);
        let alias = amplitude(&out[1000..47_000], 48_000, 18_000.0);
        assert!(alias < 0.005, "alias {alias}");
    }

    #[test]
    fn relative_ratio_changes_output_count() {
        let input = vec![0.1f32; 48_000];
        let mut rs = Resampler::new(48_000, 48_000);
        rs.set_rel(1.001);
        let mut out = Vec::new();
        rs.process(&input, &mut out);
        assert!((out.len() as i64 - 48_048).abs() < 20, "len {}", out.len());
    }
}
