//! Module de lecture audio utilisant cpal
//! 
//! Ce module implémente le trait AudioPlayback en utilisant la librairie cpal
//! pour jouer l'audio via les haut-parleurs ou casque.
//!
//! La lecture audio est plus complexe que la capture car elle nécessite :
//! - Un buffer pour gérer le jitter réseau
//! - Une gestion des underruns (pas assez de données)
//! - Une synchronisation avec l'horloge système

use async_trait::async_trait;
use cpal::{Device, Stream, SupportedStreamConfig, SampleFormat};
use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};
use tokio::sync::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::{
    AudioPlayback, AudioFrame, AudioConfig, AudioError, AudioResult,
};

/// Implémentation de lecture audio avec cpal
/// 
/// Cette structure gère :
/// - La découverte du périphérique de lecture (haut-parleurs)
/// - La configuration du stream audio de sortie
/// - Le buffering des frames pour gérer le jitter réseau
/// - La conversion de nos AudioFrame vers les échantillons cpal
/// 
/// # Architecture thread
/// 
/// Le thread principal ajoute des frames au buffer via `play_frame()`.
/// Le callback cpal (thread temps réel) lit le buffer et envoie les 
/// échantillons vers le hardware audio.
pub struct CpalPlayback {
    /// Périphérique audio de sortie (haut-parleurs)
    device: Device,
    
    /// Configuration audio de notre application
    config: AudioConfig,
    
    /// Stream audio actif (None si arrêté)
    stream: Option<Stream>,
    
    /// Buffer principal des frames en attente de lecture
    /// Protégé par un Arc<Mutex> pour accès thread-safe
    frame_buffer: Arc<Mutex<VecDeque<AudioFrame>>>,
    
    /// État de la lecture
    is_playing: bool,
    
    /// Nom du périphérique pour debug
    device_name: String,
    
    /// Compteur de frames jouées (statistiques)
    frames_played: Arc<Mutex<u64>>,
    
    /// Compteur d'underruns (manque de données)
    underruns: Arc<Mutex<u64>>,
}

impl CpalPlayback {
    /// Crée une nouvelle instance de lecture
    /// 
    /// Cette fonction découvre automatiquement le périphérique de sortie par défaut
    /// et prépare la configuration, mais ne démarre pas encore la lecture.
    /// 
    /// # Arguments
    /// * `config` - Configuration audio à utiliser
    /// 
    /// # Erreurs
    /// - `AudioError::NoDeviceFound` si aucun haut-parleur n'est disponible
    /// - `AudioError::ConfigError` si la configuration n'est pas supportée
    pub fn new(config: AudioConfig) -> AudioResult<Self> {
        // Obtient l'host audio par défaut du système
        let host = cpal::default_host();
        
        // Trouve le périphérique de sortie par défaut
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoDeviceFound)?;
            
        // Récupère le nom du périphérique pour debug
        let device_name = device.description()
            .ok()
            .map(|desc| desc.name().to_string())
            .unwrap_or_else(|| "Périphérique inconnu".to_string());
            
        // Crée le buffer avec la taille configurée
        let frame_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(
            config.receive_buffer_size * 2 // Un peu plus grand pour éviter les reallocations
        )));
        
        println!("🔊 Périphérique de lecture trouvé : {}", device_name);
        
        Ok(Self {
            device,
            config,
            stream: None,
            frame_buffer,
            is_playing: false,
            device_name,
            frames_played: Arc::new(Mutex::new(0)),
            underruns: Arc::new(Mutex::new(0)),
        })
    }
    
    /// Vérifie que la configuration audio est supportée par le périphérique
    fn validate_config(&self) -> AudioResult<SupportedStreamConfig> {
        // Obtient la configuration par défaut du périphérique
        let default_config = self.device
            .default_output_config()
            .map_err(|e| AudioError::ConfigError(format!("Impossible d'obtenir config par défaut: {}", e)))?;
        
        println!("📋 Config par défaut du périphérique de sortie :");
        println!("   Sample rate: {} Hz", default_config.sample_rate());
        println!("   Channels: {}", default_config.channels());
        println!("   Sample format: {:?}", default_config.sample_format());
        
        // Vérifie que le périphérique supporte notre sample rate
        let supported_rates = self.device
            .supported_output_configs()
            .map_err(|e| AudioError::ConfigError(format!("Impossible d'obtenir configs supportées: {}", e)))?;
        
        let mut config_found = false;
        for supported_range in supported_rates {
            let min_rate = supported_range.min_sample_rate();
            let max_rate = supported_range.max_sample_rate();
            
            if self.config.sample_rate >= min_rate && self.config.sample_rate <= max_rate {
                config_found = true;
                break;
            }
        }
        
        if !config_found {
            return Err(AudioError::ConfigError(format!(
                "Sample rate {} Hz non supporté par le périphérique de sortie", 
                self.config.sample_rate
            )));
        }
        
        
        Ok(default_config)
    }
    
    /// Construit et configure le stream audio de sortie
    fn build_stream(&mut self) -> AudioResult<Stream> {
        let stream_config = self.validate_config()?;
        
        // Clone des variables nécessaires pour le callback
        let frame_buffer = Arc::clone(&self.frame_buffer);
        let samples_per_frame = self.config.samples_per_frame();
        let frames_played = Arc::clone(&self.frames_played);
        let underruns = Arc::clone(&self.underruns);
        
        println!("🎵 Démarrage lecture :");
        println!("   Échantillons par frame : {}", samples_per_frame);
        println!("   Taille buffer : {} frames", self.config.receive_buffer_size);
        
        // Buffer local pour accumuler les échantillons
        let mut output_buffer = VecDeque::with_capacity(samples_per_frame * 4);
        
        // Détermine le format d'échantillons du périphérique
        let sample_format = stream_config.sample_format();
        
        // Construit le stream selon le format d'échantillons
        let stream = match sample_format {
            SampleFormat::F32 => {
                self.device.build_output_stream(
                    &stream_config.config(),
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        Self::fill_output_buffer_f32(
                            data,
                            &mut output_buffer,
                            &frame_buffer,
                            samples_per_frame,
                            &frames_played,
                            &underruns,
                        );
                    },
                    move |err| {
                        eprintln!("❌ Erreur stream audio sortie : {}", err);
                    },
                    None
                )?
            },
            SampleFormat::I16 => {
                self.device.build_output_stream(
                    &stream_config.config(),
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        Self::fill_output_buffer_i16(
                            data,
                            &mut output_buffer,
                            &frame_buffer,
                            samples_per_frame,
                            &frames_played,
                            &underruns,
                        );
                    },
                    move |err| {
                        eprintln!("❌ Erreur stream audio sortie : {}", err);
                    },
                    None
                )?
            },
            SampleFormat::U16 => {
                self.device.build_output_stream(
                    &stream_config.config(),
                    move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                        Self::fill_output_buffer_u16(
                            data,
                            &mut output_buffer,
                            &frame_buffer,
                            samples_per_frame,
                            &frames_played,
                            &underruns,
                        );
                    },
                    move |err| {
                        eprintln!("❌ Erreur stream audio sortie : {}", err);
                    },
                    None
                )?
            },
            _ => return Err(AudioError::ConfigError(format!("Format d'échantillon non supporté : {:?}", sample_format))),
        };
        
        Ok(stream)
    }
    
    /// Remplit le buffer de sortie avec des échantillons f32
    /// 
    /// Cette fonction est appelée par le callback audio (thread temps réel).
    /// Elle doit être très rapide et ne jamais bloquer.
    fn fill_output_buffer_f32(
        output: &mut [f32],
        sample_buffer: &mut VecDeque<f32>,
        frame_buffer: &Arc<Mutex<VecDeque<AudioFrame>>>,
        _samples_per_frame: usize,
        frames_played: &Arc<Mutex<u64>>,
        underruns: &Arc<Mutex<u64>>,
    ) {
        // Remplit le buffer d'échantillons si nécessaire
        while sample_buffer.len() < output.len() {
            // Essaie de récupérer une frame (non-bloquant)
            if let Ok(mut buffer_guard) = frame_buffer.try_lock() {
                if let Some(frame) = buffer_guard.pop_front() {
                    // Ajoute tous les échantillons de cette frame
                    for sample in frame.samples {
                        sample_buffer.push_back(sample);
                    }
                    
                    // Met à jour les statistiques (non-bloquant)
                    if let Ok(mut count) = frames_played.try_lock() {
                        *count += 1;
                    }
                } else {
                    // Pas de frame disponible - underrun
                    if let Ok(mut count) = underruns.try_lock() {
                        *count += 1;
                    }
                    break;
                }
            } else {
                // Impossible d'obtenir le lock - on continue avec ce qu'on a
                break;
            }
        }
        
        // Remplit la sortie avec les échantillons disponibles
        for sample in output.iter_mut() {
            *sample = sample_buffer.pop_front().unwrap_or(0.0); // Silence si pas de données
        }
    }
    
    /// Remplit le buffer de sortie avec des échantillons i16 (conversion depuis f32)
    fn fill_output_buffer_i16(
        output: &mut [i16],
        sample_buffer: &mut VecDeque<f32>,
        frame_buffer: &Arc<Mutex<VecDeque<AudioFrame>>>,
        _samples_per_frame: usize,
        frames_played: &Arc<Mutex<u64>>,
        underruns: &Arc<Mutex<u64>>,
    ) {
        // Même logique que f32, mais on convertit en remplissant
        while sample_buffer.len() < output.len() {
            if let Ok(mut buffer_guard) = frame_buffer.try_lock() {
                if let Some(frame) = buffer_guard.pop_front() {
                    for sample in frame.samples {
                        sample_buffer.push_back(sample);
                    }
                    
                    if let Ok(mut count) = frames_played.try_lock() {
                        *count += 1;
                    }
                } else {
                    if let Ok(mut count) = underruns.try_lock() {
                        *count += 1;
                    }
                    break;
                }
            } else {
                break;
            }
        }
        
        // Remplit et convertit f32 -> i16
        for sample in output.iter_mut() {
            let f32_sample = sample_buffer.pop_front().unwrap_or(0.0);
            // Convertit f32 [-1.0, 1.0] vers i16
            *sample = (f32_sample * i16::MAX as f32) as i16;
        }
    }
    
    /// Remplit le buffer de sortie avec des échantillons u16 (conversion depuis f32)
    fn fill_output_buffer_u16(
        output: &mut [u16],
        sample_buffer: &mut VecDeque<f32>,
        frame_buffer: &Arc<Mutex<VecDeque<AudioFrame>>>,
        _samples_per_frame: usize,
        frames_played: &Arc<Mutex<u64>>,
        underruns: &Arc<Mutex<u64>>,
    ) {
        // Même logique que f32, mais on convertit en remplissant
        while sample_buffer.len() < output.len() {
            if let Ok(mut buffer_guard) = frame_buffer.try_lock() {
                if let Some(frame) = buffer_guard.pop_front() {
                    for sample in frame.samples {
                        sample_buffer.push_back(sample);
                    }
                    
                    if let Ok(mut count) = frames_played.try_lock() {
                        *count += 1;
                    }
                } else {
                    if let Ok(mut count) = underruns.try_lock() {
                        *count += 1;
                    }
                    break;
                }
            } else {
                break;
            }
        }
        
        // Remplit et convertit f32 -> u16
        for sample in output.iter_mut() {
            let f32_sample = sample_buffer.pop_front().unwrap_or(0.0);
            // Convertit f32 [-1.0, 1.0] vers u16 [0, 65535]
            *sample = ((f32_sample + 1.0) * 0.5 * u16::MAX as f32) as u16;
        }
    }
    
    /// Retourne les statistiques de lecture
    pub async fn get_stats(&self) -> (u64, u64) {
        let frames = *self.frames_played.lock().await;
        let underruns = *self.underruns.lock().await;
        (frames, underruns)
    }
}

#[async_trait]
impl AudioPlayback for CpalPlayback {
    async fn start(&mut self) -> AudioResult<()> {
        if self.is_playing {
            return Ok(()); // Déjà démarré
        }
        
        println!("🚀 Démarrage de la lecture audio...");
        
        // Construit et démarre le stream
        let stream = self.build_stream()?;
        stream.play()?;
        
        self.stream = Some(stream);
        self.is_playing = true;
        
        println!("✅ Lecture audio démarrée");
        Ok(())
    }
    
    async fn stop(&mut self) -> AudioResult<()> {
        if !self.is_playing {
            return Ok(()); // Déjà arrêté
        }
        
        println!("🛑 Arrêt de la lecture audio...");
        
        // Arrête et supprime le stream
        if let Some(stream) = self.stream.take() {
            stream.pause()?;
        }
        
        self.is_playing = false;
        
        println!("✅ Lecture audio arrêtée");
        Ok(())
    }
    
    async fn play_frame(&mut self, frame: AudioFrame) -> AudioResult<()> {
        let mut buffer_guard = self.frame_buffer.lock().await;
        
        // Vérifie si le buffer est plein
        if buffer_guard.len() >= self.config.receive_buffer_size {
            // Buffer plein - on peut soit dropper la frame la plus ancienne,
            // soit rejeter la nouvelle frame
            buffer_guard.pop_front(); // Drop la plus ancienne
            return Err(AudioError::BufferOverflow);
        }
        
        // Ajoute la frame au buffer
        buffer_guard.push_back(frame);
        Ok(())
    }
    
    fn is_playing(&self) -> bool {
        self.is_playing
    }
    
    fn buffer_level(&self) -> usize {
        // Note: try_lock pour éviter de bloquer si appelé depuis un callback
        if let Ok(buffer_guard) = self.frame_buffer.try_lock() {
            buffer_guard.len()
        } else {
            0 // Estimation si on ne peut pas lock
        }
    }
    
    async fn flush_buffer(&mut self) -> AudioResult<()> {
        let mut buffer_guard = self.frame_buffer.lock().await;
        buffer_guard.clear();
        println!("🗑️  Buffer de lecture vidé");
        Ok(())
    }
    
    fn device_info(&self) -> String {
        self.device_name.clone()
    }
}

// Implémentation de Drop pour nettoyer proprement
impl Drop for CpalPlayback {
    fn drop(&mut self) {
        if self.is_playing {
            println!("🧹 Nettoyage automatique de la lecture audio");
            // Note: on ne peut pas appeler stop() ici car c'est async
            // Le stream sera automatiquement arrêté quand il sera dropped
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[test]
    fn test_playback_creation() {
        let config = AudioConfig::default();
        
        // Test que la création ne panic pas
        match CpalPlayback::new(config) {
            Ok(playback) => {
                assert!(!playback.is_playing());
                assert!(!playback.device_info().is_empty());
                assert_eq!(playback.buffer_level(), 0);
            },
            Err(AudioError::NoDeviceFound) => {
                println!("⚠️  Pas de haut-parleur disponible pour le test");
            },
            Err(e) => panic!("Erreur inattendue: {}", e),
        }
    }
    
    #[tokio::test]
    async fn test_playback_start_stop() {
        let config = AudioConfig::default();
        
        if let Ok(mut playback) = CpalPlayback::new(config) {
            // Test start/stop basique
            assert!(!playback.is_playing());
            
            if playback.start().await.is_ok() {
                assert!(playback.is_playing());
                
                if playback.stop().await.is_ok() {
                    assert!(!playback.is_playing());
                }
            }
        }
    }
    
    #[tokio::test]
    async fn test_playback_buffer() {
        let config = AudioConfig::default();
        
        if let Ok(mut playback) = CpalPlayback::new(config.clone()) {
            assert_eq!(playback.buffer_level(), 0);
            
            // Ajoute des frames au buffer
            for i in 0..3 {
                let frame = AudioFrame::silence(config.samples_per_frame(), i);
                if playback.play_frame(frame).await.is_ok() {
                    assert_eq!(playback.buffer_level(), (i + 1) as usize);
                }
            }
            
            // Test flush
            if playback.flush_buffer().await.is_ok() {
                assert_eq!(playback.buffer_level(), 0);
            }
        }
    }
    
    #[tokio::test]
    async fn test_playback_buffer_overflow() {
        let config = AudioConfig::default();
        
        if let Ok(mut playback) = CpalPlayback::new(config.clone()) {
            // Remplit le buffer au maximum
            for i in 0..config.receive_buffer_size {
                let frame = AudioFrame::silence(config.samples_per_frame(), i as u64);
                let result = playback.play_frame(frame).await;
                assert!(result.is_ok());
            }
            
            // Une frame de plus doit causer un overflow
            let overflow_frame = AudioFrame::silence(config.samples_per_frame(), 999);
            let result = playback.play_frame(overflow_frame).await;
            assert!(matches!(result, Err(AudioError::BufferOverflow)));
        }
    }
    
    // Note: Ce test nécessite de vrais haut-parleurs et peut être audible
    #[tokio::test]
    #[ignore] // Ignore par défaut, lance avec --ignored pour tester
    async fn test_playback_audio() {
        let config = AudioConfig::default();
        
        if let Ok(mut playback) = CpalPlayback::new(config.clone()) {
            if playback.start().await.is_ok() {
                println!("🔊 Test audio en cours - vous devriez entendre des bips...");
                
                // Génère et joue plusieurs bips
                for freq in &[440.0, 523.0, 659.0] { // Do, Mi, Sol
                    let samples_per_frame = config.samples_per_frame();
                    let sample_rate = config.sample_rate as f32;
                    
                    // Génère un bip de 100ms
                    for frame_idx in 0..5 { // 5 frames * 20ms = 100ms
                        let mut beep_samples = Vec::with_capacity(samples_per_frame);
                        for i in 0..samples_per_frame {
                            let t = (frame_idx * samples_per_frame + i) as f32 / sample_rate;
                            let sample = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.3;
                            beep_samples.push(sample);
                        }
                        
                        let beep_frame = AudioFrame::new(beep_samples, frame_idx as u64);
                        if playback.play_frame(beep_frame).await.is_err() {
                            break;
                        }
                    }
                    
                    // Pause entre les bips
                    sleep(Duration::from_millis(200)).await;
                }
                
                // Attend que tout soit joué
                sleep(Duration::from_millis(500)).await;
                
                let (frames_played, underruns) = playback.get_stats().await;
                println!("📊 Statistiques lecture :");
                println!("   Frames jouées : {}", frames_played);
                println!("   Underruns : {}", underruns);
                
                let _ = playback.stop().await;
            }
        }
    }
}
