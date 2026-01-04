//! Types de données pour le système audio
//! 
//! Ce module définit les structures principales pour manipuler l'audio :
//! - AudioFrame : Frame audio brute (échantillons non compressés)
//! - CompressedFrame : Frame audio compressée avec Opus
//! - Sample : Type pour un échantillon audio

use std::time::Instant;
use serde::{Deserialize, Serialize};

/// Type pour un échantillon audio
/// 
/// Un échantillon représente l'amplitude du son à un instant donné.
/// - Valeurs entre -1.0 et +1.0  
/// - 0.0 = silence
/// - +1.0 = amplitude maximale positive
/// - -1.0 = amplitude maximale négative
/// - f32 est suffisant pour la qualité audio (24 bits effectifs)
pub type Sample = f32;

/// Frame d'audio brute (non compressée)
/// 
/// Une frame contient un petit morceau d'audio (typiquement 20ms).
/// C'est l'unité de base pour tout le traitement audio.
/// 
/// Exemple pour 20ms à 48kHz mono :
/// - 960 échantillons (48000 * 0.02)
/// - ~3840 bytes (960 * 4 bytes par f32)
#[derive(Clone, Debug, PartialEq)]
pub struct AudioFrame {
    /// Les échantillons audio bruts
    /// 
    /// Pour du mono : Vec de taille samples_per_frame
    /// Pour du stéréo : échantillons entrelacés [L, R, L, R, ...]
    pub samples: Vec<Sample>,
    
    /// Timestamp de création de cette frame
    /// 
    /// Utilisé pour :
    /// - Mesurer la latence
    /// - Synchroniser l'audio
    /// - Détecter les frames trop anciennes
    pub timestamp: Instant,
    
    /// Numéro de séquence pour détecter les frames perdues
    /// 
    /// Incrémenté pour chaque frame envoyée.
    /// Permet de détecter si des frames sont perdues sur le réseau.
    pub sequence_number: u64,
}

impl AudioFrame {
    /// Crée une nouvelle frame audio
    /// 
    /// # Arguments
    /// * `samples` - Les échantillons audio
    /// * `sequence_number` - Numéro de séquence unique
    /// 
    /// # Example
    /// ```rust
    /// use audio::AudioFrame;
    /// 
    /// let samples = vec![0.1, 0.2, -0.1, 0.0]; // 4 échantillons
    /// let frame = AudioFrame::new(samples, 42);
    /// ```
    pub fn new(samples: Vec<Sample>, sequence_number: u64) -> Self {
        Self {
            samples,
            timestamp: Instant::now(),
            sequence_number,
        }
    }
    
    /// Crée une frame de silence
    /// 
    /// Utile pour combler les trous quand on perd des frames réseau
    pub fn silence(sample_count: usize, sequence_number: u64) -> Self {
        Self::new(vec![0.0; sample_count], sequence_number)
    }
    
    /// Calcule la durée de cette frame en millisecondes
    /// 
    /// Basé sur le nombre d'échantillons et un sample rate supposé de 48kHz
    pub fn duration_ms(&self) -> f32 {
        (self.samples.len() as f32 / 48000.0) * 1000.0
    }
    
    /// Vérifie si cette frame est essentiellement silencieuse
    /// 
    /// Utile pour optimiser (ne pas envoyer les frames silencieuses)
    pub fn is_silence(&self, threshold: f32) -> bool {
        self.samples.iter().all(|&sample| sample.abs() < threshold)
    }
    
    /// Calcule le niveau sonore RMS (Root Mean Square)
    /// 
    /// RMS donne une idée du volume moyen de la frame.
    /// Retourne une valeur entre 0.0 et 1.0
    pub fn rms_level(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        
        let sum_squares: f32 = self.samples.iter()
            .map(|&s| s * s)
            .sum();
            
        (sum_squares / self.samples.len() as f32).sqrt()
    }
    
    /// Calcule le niveau maximum (peak)
    /// 
    /// Retourne l'échantillon avec l'amplitude la plus élevée
    pub fn peak_level(&self) -> f32 {
        self.samples.iter()
            .map(|&s| s.abs())
            .fold(0.0, f32::max)
    }
    
    /// Applique un gain (amplification/atténuation) à la frame
    /// 
    /// # Arguments  
    /// * `gain` - Facteur de multiplication (1.0 = pas de changement, 0.5 = -6dB, 2.0 = +6dB)
    pub fn apply_gain(&mut self, gain: f32) {
        for sample in &mut self.samples {
            *sample *= gain;
            // Écrêtage pour éviter la saturation
            *sample = sample.clamp(-1.0, 1.0);
        }
    }
    
    /// Mélange cette frame avec une autre (additionne les échantillons)
    /// 
    /// Utile pour mixer plusieurs sources audio
    pub fn mix_with(&mut self, other: &AudioFrame) {
        let min_len = self.samples.len().min(other.samples.len());
        
        for i in 0..min_len {
            self.samples[i] += other.samples[i];
            // Écrêtage pour éviter la saturation
            self.samples[i] = self.samples[i].clamp(-1.0, 1.0);
        }
    }
}

/// Frame d'audio compressée avec Opus
/// 
/// Après compression, l'audio prend beaucoup moins de place :
/// - Frame brute : ~3840 bytes (20ms à 48kHz mono)  
/// - Frame compressée : ~80-200 bytes (ratio ~20:1)
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CompressedFrame {
    /// Données compressées par Opus
    /// 
    /// Format binaire opaque - seul Opus peut le décoder
    pub data: Vec<u8>,
    
    /// Nombre d'échantillons dans la frame originale
    /// 
    /// Nécessaire pour reconstruire une AudioFrame de la bonne taille
    pub original_sample_count: usize,
    
    /// Timestamp de création (avant compression)
    #[serde(skip)]
    pub timestamp: Instant,
    
    /// Numéro de séquence de la frame originale
    pub sequence_number: u64,
}

impl Default for CompressedFrame {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            original_sample_count: 0,
            timestamp: Instant::now(),
            sequence_number: 0,
        }
    }
}

impl CompressedFrame {
    /// Crée une nouvelle frame compressée
    pub fn new(
        data: Vec<u8>, 
        original_sample_count: usize, 
        timestamp: Instant, 
        sequence_number: u64
    ) -> Self {
        Self {
            data,
            original_sample_count,
            timestamp,
            sequence_number,
        }
    }
    
    /// Calcule le ratio de compression obtenu
    /// 
    /// Exemple : ratio de 20.0 = la frame compressée fait 20x moins que l'originale
    pub fn compression_ratio(&self) -> f32 {
        let original_size_bytes = self.original_sample_count * 4; // f32 = 4 bytes
        if self.data.is_empty() {
            return 1.0;
        }
        original_size_bytes as f32 / self.data.len() as f32
    }
    
    /// Calcule l'âge de cette frame (temps écoulé depuis la création)
    pub fn age(&self) -> std::time::Duration {
        self.timestamp.elapsed()
    }
    
    /// Vérifie si cette frame est "trop vieille" pour être utilisée
    /// 
    /// Si une frame arrive très en retard, il vaut mieux la jeter
    pub fn is_stale(&self, max_age_ms: u32) -> bool {
        self.age().as_millis() > max_age_ms as u128
    }
}

/// Statistiques audio pour le monitoring
/// 
/// Permet de surveiller la qualité et les performances du système audio
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AudioStats {
    /// Nombre de frames capturées
    pub frames_captured: u64,
    
    /// Nombre de frames jouées
    pub frames_played: u64,
    
    /// Nombre de frames perdues (network)
    pub frames_lost: u64,
    
    /// Niveau RMS moyen des dernières frames
    pub avg_rms_level: f32,
    
    /// Latence moyenne mesurée (ms)
    pub avg_latency_ms: f32,
    
    /// Ratio de compression moyen
    pub avg_compression_ratio: f32,
    
    /// Nombre de buffer overflows/underruns
    pub buffer_overflows: u64,
    pub buffer_underruns: u64,
}

impl AudioStats {
    /// Remet les statistiques à zéro
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    
    /// Calcule le pourcentage de frames perdues
    pub fn loss_percentage(&self) -> f32 {
        if self.frames_captured == 0 {
            return 0.0;
        }
        (self.frames_lost as f32 / self.frames_captured as f32) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_audio_frame_creation() {
        let samples = vec![0.1, -0.2, 0.3, 0.0];
        let frame = AudioFrame::new(samples.clone(), 42);
        
        assert_eq!(frame.samples, samples);
        assert_eq!(frame.sequence_number, 42);
        assert!(frame.timestamp.elapsed().as_millis() < 100); // Créé récemment
    }
    
    #[test]
    fn test_silence_detection() {
        let silent = AudioFrame::new(vec![0.0, 0.001, -0.001, 0.0], 1);
        let noisy = AudioFrame::new(vec![0.1, 0.5, -0.3, 0.2], 2);
        
        assert!(silent.is_silence(0.01));
        assert!(!noisy.is_silence(0.01));
    }
    
    #[test]
    fn test_rms_calculation() {
        // Frame avec échantillons connus
        let samples = vec![0.5, -0.5, 0.5, -0.5];
        let frame = AudioFrame::new(samples, 1);
        
        let rms = frame.rms_level();
        assert!((rms - 0.5).abs() < 0.001); // RMS de ±0.5 = 0.5
    }
    
    #[test]
    fn test_gain_application() {
        let mut frame = AudioFrame::new(vec![0.5, -0.5, 0.8], 1);
        frame.apply_gain(2.0); // Double le volume
        
        assert_eq!(frame.samples[0], 1.0);  // 0.5 * 2 = 1.0
        assert_eq!(frame.samples[1], -1.0); // -0.5 * 2 = -1.0
        assert_eq!(frame.samples[2], 1.0);  // 0.8 * 2 = 1.6 -> clamped à 1.0
    }
    
    #[test]
    fn test_compression_ratio() {
        let compressed = CompressedFrame::new(
            vec![1, 2, 3, 4], // 4 bytes compressés
            960,              // 960 échantillons originaux
            Instant::now(),
            1
        );
        
        let expected_ratio = (960 * 4) as f32 / 4.0; // 3840 / 4 = 960.0
        assert_eq!(compressed.compression_ratio(), expected_ratio);
    }
    
    #[test]
    fn test_stats_loss_percentage() {
        let mut stats = AudioStats::default();
        stats.frames_captured = 100;
        stats.frames_lost = 5;
        
        assert_eq!(stats.loss_percentage(), 5.0);
    }
}
