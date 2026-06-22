//! Analyseur de spectre par bandes, faible coût CPU.
//!
//! FFT radix-2 itérative écrite à la main (aucune dépendance), appliquée à une
//! fenêtre glissante de [`FFT_SIZE`] échantillons mono, regroupée en [`BANDS`]
//! bandes log-espacées. Le coût est négligeable : une FFT de 1024 points
//! quelques dizaines de fois par seconde (le moteur la limite dans le temps).

use std::f32::consts::PI;

/// Taille de la fenêtre FFT (puissance de deux).
pub const FFT_SIZE: usize = 1024;
/// Nombre de bandes affichées.
pub const BANDS: usize = 24;

/// FFT itérative en place (Cooley-Tukey, entrées réelles dans `re`, `im=0`).
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two());
    debug_assert_eq!(im.len(), n);

    // Permutation par inversion de bits.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Papillons.
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut base = 0;
        while base < n {
            let (mut cwr, mut cwi) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let a = base + k;
                let b = base + k + len / 2;
                let tr = cwr * re[b] - cwi * im[b];
                let ti = cwr * im[b] + cwi * re[b];
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncwr = cwr * wr - cwi * wi;
                cwi = cwr * wi + cwi * wr;
                cwr = ncwr;
            }
            base += len;
        }
        len <<= 1;
    }
}

/// Borne basse (indice de bin) de la bande `b`, répartition log de 1 à `half`.
fn band_edge(b: usize, half: usize) -> usize {
    let min = 1.0f32;
    let max = half as f32;
    let t = b as f32 / BANDS as f32;
    (min * (max / min).powf(t)) as usize
}

/// Calcule les [`BANDS`] amplitudes (0..1) à partir d'une fenêtre mono.
///
/// Les échantillons sont supposés dans `[-1, 1]`. On applique une fenêtre de
/// Hann, une FFT, puis on agrège les magnitudes par bande log et on compresse
/// en échelle logarithmique pour un rendu lisible.
pub fn compute_bands(samples: &[f32]) -> [f32; BANDS] {
    let n = FFT_SIZE;
    let mut re = vec![0.0f32; n];
    let mut im = vec![0.0f32; n];

    let len = samples.len().min(n);
    // On fenêtre les `len` derniers échantillons (alignés au début du buffer).
    for i in 0..len {
        let w = 0.5 - 0.5 * (2.0 * PI * i as f32 / (n as f32 - 1.0)).cos();
        re[i] = samples[i] * w;
    }

    fft(&mut re, &mut im);

    let half = n / 2;
    let mut bands = [0.0f32; BANDS];
    for b in 0..BANDS {
        let lo = band_edge(b, half).max(1);
        let hi = band_edge(b + 1, half).max(lo + 1).min(half);
        let mut peak = 0.0f32;
        for k in lo..hi {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            if mag > peak {
                peak = mag;
            }
        }
        // Compression log : ~ dB normalisé. Constante réglée pour la musique.
        let v = (1.0 + peak).ln() / 7.0;
        bands[b] = v.clamp(0.0, 1.0);
    }
    bands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_d_un_dirac_est_plat() {
        let mut re = vec![0.0f32; 8];
        let mut im = vec![0.0f32; 8];
        re[0] = 1.0;
        fft(&mut re, &mut im);
        // FFT d'une impulsion = spectre constant.
        for k in 0..8 {
            assert!((re[k] - 1.0).abs() < 1e-4);
            assert!(im[k].abs() < 1e-4);
        }
    }

    #[test]
    fn un_sinus_pique_dans_la_bonne_bande() {
        // Sinus à un bin précis : doit produire un maximum dans une bande haute,
        // pas dans les graves.
        let n = FFT_SIZE;
        let bin = 200.0; // fréquence relativement haute
        let samples: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * bin * i as f32 / n as f32).sin())
            .collect();
        let bands = compute_bands(&samples);
        let argmax = bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        // bin 200 sur 512 → partie haute du spectre.
        assert!(argmax > BANDS / 2, "pic attendu dans les aigus, eu {argmax}");
        // Toutes les valeurs restent normalisées.
        assert!(bands.iter().all(|&v| (0.0..=1.0).contains(&v)));
    }

    #[test]
    fn silence_donne_des_bandes_nulles() {
        let bands = compute_bands(&vec![0.0f32; FFT_SIZE]);
        assert!(bands.iter().all(|&v| v < 1e-3));
    }
}
