//! Module de capture audio utilisant cpal
//! 
//! Ce module implémente le trait AudioCapture en utilisant la librairie cpal
//! (Cross-Platform Audio Library) pour capturer l'audio depuis le microphone.
//!
//! cpal est la librairie standard en Rust pour l'audio cross-platform.
//! Elle supporte Windows (WASAPI), macOS (CoreAudio), et Linux (ALSA/PulseAudio).

use async_trait::async_trait;
use cpal::{Device, Stream, SupportedStreamConfig, SampleFormat};
use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use std::sync::Arc;

use crate::{
    AudioCapture, AudioFrame, AudioConfig, AudioError, AudioResult,
};

/// Implémentation de capture audio avec cpal
/// 
/// Cette structure gère :
/// - La découverte du périphérique de capture (microphone)
/// - La configuration du stream audio
/// - La conversion des échantillons cpal vers nos AudioFrame
/// - Le buffering des frames pour éviter les pertes
/// 
/// # Architecture thread
/// 
/// cpal fonctionne avec des callbacks. Quand des données audio arrivent,
/// cpal appelle notre fonction qui accumule les échantillons.
/// Quand on a assez d'échantillons pour une frame (20ms), on l'envoie
/// via un channel async vers le thread principal.
pub struct CpalCapture {
    /// Périphérique audio d'entrée (microphone)
    device: Device,
    
    /// Configuration audio de notre application
    config: AudioConfig,
    
    /// Stream audio actif (None si arrêté)
    stream: Option<Stream>,
    
    /// Channel pour recevoir les frames depuis le callback cpal
    frame_receiver: Arc<Mutex<Option<mpsc::Receiver<AudioFrame>>>>,
    
    /// Sender pour envoyer des frames depuis le callback (clone dans le callback)
    frame_sender: Option<mpsc::Sender<AudioFrame>>,
    
    /// État de l'enregistrement
    is_recording: bool,
    
    /// Compteur de séquence pour les frames
    sequence_counter: Arc<Mutex<u64>>,
    
    /// Nom du périphérique pour debug
    device_name: String,
}

impl CpalCapture {
    /// Crée une nouvelle instance de capture
    /// 
    /// Cette fonction découvre automatiquement le périphérique d'entrée par défaut
    /// et prépare la configuration, mais ne démarre pas encore la capture.
    /// 
    /// # Arguments
    /// * `config` - Configuration audio à utiliser
    /// 
    /// # Erreurs
    /// - `AudioError::NoDeviceFound` si aucun microphone n'est disponible
    /// - `AudioError::ConfigError` si la configuration n'est pas supportée
    pub fn new(config: AudioConfig) -> AudioResult<Self> {
        // Obtient l'host audio par défaut du système
        let host = cpal::default_host();
        
        // Trouve le périphérique d'entrée par défaut
        let device = host
            .default_input_device()
            .ok_or(AudioError::NoDeviceFound)?;
            
        // Récupère la description du périphérique pour debug
        // description() remplace name() et fournit des informations plus complètes
        let device_name = device.description()
            .ok()
            .map(|desc| desc.name().to_string())
            .unwrap_or_else(|| "Périphérique inconnu".to_string());
            
        // Crée le channel pour communiquer entre le callback et async
        let (frame_sender, frame_receiver) = mpsc::channel(10);
        
        println!("🎤 Périphérique de capture trouvé : {}", device_name);
        
        Ok(Self {
            device,
            config,
            stream: None,
            frame_receiver: Arc::new(Mutex::new(Some(frame_receiver))),
            frame_sender: Some(frame_sender),
            is_recording: false,
            sequence_counter: Arc::new(Mutex::new(0)),
            device_name,
        })
    }
    
    /// Vérifie que la configuration audio est supportée par le périphérique
    /// 
    /// Cette fonction valide que le périphérique peut capturer avec nos paramètres.
    fn validate_config(&self) -> AudioResult<SupportedStreamConfig> {
        // Obtient la configuration par défaut du périphérique
        let default_config = self.device
            .default_input_config()
            .map_err(|e| AudioError::ConfigError(format!("Impossible d'obtenir config par défaut: {}", e)))?;
        
        println!("📋 Config par défaut du périphérique :");
        println!("   Sample rate: {} Hz", default_config.sample_rate());
        println!("   Channels: {}", default_config.channels());
        println!("   Sample format: {:?}", default_config.sample_format());
        
        // Vérifie que le périphérique supporte notre sample rate
        let supported_rates = self.device
            .supported_input_configs()
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
                "Sample rate {} Hz non supporté par le périphérique", 
                self.config.sample_rate
            )));
        }
        
        // Utilise la configuration par défaut avec nos paramètres si possible
        // Pour l'instant, on accepte la config du périphérique et on adapte notre côté
        println!("✅ Configuration validée - utilise la config par défaut");
        
        Ok(default_config)
    }
    
    /// Construit et configure le stream audio
    fn build_stream(&mut self) -> AudioResult<Stream> {
        let stream_config = self.validate_config()?;
        
        // Clone des variables nécessaires pour le callback
        let sender = self.frame_sender.as_ref().unwrap().clone();
        let samples_per_frame = self.config.samples_per_frame();
        let sequence_counter = Arc::clone(&self.sequence_counter);
        
        println!("🎵 Démarrage capture :");
        println!("   Échantillons par frame : {}", samples_per_frame);
        println!("   Durée par frame : {}ms", self.config.frame_duration_ms);
        
        // Buffer pour accumuler les échantillons
        let mut sample_buffer = Vec::with_capacity(samples_per_frame);
        
        // Détermine le format d'échantillons du périphérique
        let sample_format = stream_config.sample_format();
        
        // Construit le stream selon le format d'échantillons
        let stream = match sample_format {
            SampleFormat::F32 => {
                self.device.build_input_stream(
                    &stream_config.config(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        Self::process_samples_f32(
                            data, 
                            &mut sample_buffer, 
                            samples_per_frame,
                            &sender,
                            &sequence_counter
                        );
                    },
                    move |err| {
                        eprintln!("❌ Erreur stream audio : {}", err);
                    },
                    None
                )?
            },
            SampleFormat::I16 => {
                self.device.build_input_stream(
                    &stream_config.config(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        Self::process_samples_i16(
                            data, 
                            &mut sample_buffer, 
                            samples_per_frame,
                            &sender,
                            &sequence_counter
                        );
                    },
                    move |err| {
                        eprintln!("❌ Erreur stream audio : {}", err);
                    },
                    None
                )?
            },
            SampleFormat::U16 => {
                self.device.build_input_stream(
                    &stream_config.config(),
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        Self::process_samples_u16(
                            data, 
                            &mut sample_buffer, 
                            samples_per_frame,
                            &sender,
                            &sequence_counter
                        );
                    },
                    move |err| {
                        eprintln!("❌ Erreur stream audio : {}", err);
                    },
                    None
                )?
            },
            _ => return Err(AudioError::ConfigError(format!("Format d'échantillon non supporté : {:?}", sample_format))),
        };
        
        Ok(stream)
    }
    
    /// Traite les échantillons f32 depuis cpal
    /// 
    /// Cette fonction est appelée dans le callback audio (thread temps réel).
    /// Elle doit être très rapide pour éviter les coupures.
    fn process_samples_f32(
        data: &[f32],
        sample_buffer: &mut Vec<f32>,
        samples_per_frame: usize,
        sender: &mpsc::Sender<AudioFrame>,
        sequence_counter: &Arc<Mutex<u64>>,
    ) {
        for &sample in data {
            sample_buffer.push(sample);
            
            // Si on a assez d'échantillons pour une frame
            if sample_buffer.len() >= samples_per_frame {
                // Obtient le numéro de séquence (non-bloquant)
                let sequence = if let Ok(mut counter) = sequence_counter.try_lock() {
                    let seq = *counter;
                    *counter += 1;
                    seq
                } else {
                    0 // Fallback si le lock échoue (rare)
                };
                
                // Crée la frame audio
                let frame = AudioFrame::new(
                    sample_buffer.drain(..).collect(),
                    sequence
                );
                
                // Envoie la frame (non-bloquant)
                if let Err(_) = sender.try_send(frame) {
                    // Le buffer est plein - on perd cette frame
                    // C'est normal sous charge, ne pas panic
                }
            }
        }
    }
    
    /// Traite les échantillons i16 depuis cpal (conversion vers f32)
    fn process_samples_i16(
        data: &[i16],
        sample_buffer: &mut Vec<f32>,
        samples_per_frame: usize,
        sender: &mpsc::Sender<AudioFrame>,
        sequence_counter: &Arc<Mutex<u64>>,
    ) {
        for &sample in data {
            // Convertit i16 vers f32 (plage [-1.0, 1.0])
            let f32_sample = sample as f32 / i16::MAX as f32;
            sample_buffer.push(f32_sample);
            
            if sample_buffer.len() >= samples_per_frame {
                let sequence = if let Ok(mut counter) = sequence_counter.try_lock() {
                    let seq = *counter;
                    *counter += 1;
                    seq
                } else {
                    0
                };
                
                let frame = AudioFrame::new(
                    sample_buffer.drain(..).collect(),
                    sequence
                );
                
                let _ = sender.try_send(frame);
            }
        }
    }
    
    /// Traite les échantillons u16 depuis cpal (conversion vers f32)
    fn process_samples_u16(
        data: &[u16],
        sample_buffer: &mut Vec<f32>,
        samples_per_frame: usize,
        sender: &mpsc::Sender<AudioFrame>,
        sequence_counter: &Arc<Mutex<u64>>,
    ) {
        for &sample in data {
            // Convertit u16 vers f32 (plage [-1.0, 1.0])
            let f32_sample = (sample as f32 / u16::MAX as f32) * 2.0 - 1.0;
            sample_buffer.push(f32_sample);
            
            if sample_buffer.len() >= samples_per_frame {
                let sequence = if let Ok(mut counter) = sequence_counter.try_lock() {
                    let seq = *counter;
                    *counter += 1;
                    seq
                } else {
                    0
                };
                
                let frame = AudioFrame::new(
                    sample_buffer.drain(..).collect(),
                    sequence
                );
                
                let _ = sender.try_send(frame);
            }
        }
    }
}

#[async_trait]
impl AudioCapture for CpalCapture {
    async fn start(&mut self) -> AudioResult<()> {
        if self.is_recording {
            return Ok(()); // Déjà démarré
        }
        
        println!("🚀 Démarrage de la capture audio...");
        
        // Construit et démarre le stream
        let stream = self.build_stream()?;
        stream.play()?;
        
        self.stream = Some(stream);
        self.is_recording = true;
        
        println!("✅ Capture audio démarrée");
        Ok(())
    }
    
    async fn stop(&mut self) -> AudioResult<()> {
        if !self.is_recording {
            return Ok(()); // Déjà arrêté
        }
        
        println!("🛑 Arrêt de la capture audio...");
        
        // Arrête et supprime le stream
        if let Some(stream) = self.stream.take() {
            stream.pause()?;
        }
        
        self.is_recording = false;
        
        println!("✅ Capture audio arrêtée");
        Ok(())
    }
    
    async fn next_frame(&mut self) -> AudioResult<AudioFrame> {
        // Récupère le receiver depuis le mutex
        let mut receiver_guard = self.frame_receiver.lock().await;
        let receiver = receiver_guard.as_mut()
            .ok_or(AudioError::InitializationError("Receiver non initialisé".to_string()))?;
        
        // Attend la prochaine frame
        match receiver.recv().await {
            Some(frame) => Ok(frame),
            None => Err(AudioError::DeviceDisconnected),
        }
    }
    
    fn is_recording(&self) -> bool {
        self.is_recording
    }
    
    fn device_info(&self) -> String {
        self.device_name.clone()
    }
}

// Implémentation de Drop pour nettoyer proprement
impl Drop for CpalCapture {
    fn drop(&mut self) {
        if self.is_recording {
            println!("🧹 Nettoyage automatique de la capture audio");
            // Note: on ne peut pas appeler stop() ici car c'est async
            // Le stream sera automatiquement arrêté quand il sera dropped
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};
    
    #[test]
    fn test_capture_creation() {
        let config = AudioConfig::default();
        
        // Test que la création ne panic pas
        // Note: peut échouer si aucun microphone n'est disponible
        match CpalCapture::new(config) {
            Ok(capture) => {
                assert!(!capture.is_recording());
                assert!(!capture.device_info().is_empty());
            },
            Err(AudioError::NoDeviceFound) => {
                // Acceptable dans un environnement de test sans audio
                println!("⚠️  Pas de microphone disponible pour le test");
            },
            Err(e) => panic!("Erreur inattendue: {}", e),
        }
    }
    
    #[tokio::test]
    async fn test_capture_start_stop() {
        let config = AudioConfig::default();
        
        if let Ok(mut capture) = CpalCapture::new(config) {
            // Test start/stop basique
            assert!(!capture.is_recording());
            
            if capture.start().await.is_ok() {
                assert!(capture.is_recording());
                
                if capture.stop().await.is_ok() {
                    assert!(!capture.is_recording());
                }
            }
        }
    }
    
    // Note: Ce test nécessite un vrai microphone et peut être lent
    #[tokio::test]
    #[ignore] // Ignore par défaut, lance avec --ignored pour tester
    async fn test_capture_frame() {
        let config = AudioConfig::default();
        
        if let Ok(mut capture) = CpalCapture::new(config) {
            if capture.start().await.is_ok() {
                // Essaie de récupérer une frame dans les 5 secondes
                match timeout(Duration::from_secs(5), capture.next_frame()).await {
                    Ok(Ok(frame)) => {
                        assert_eq!(frame.samples.len(), 960); // 20ms à 48kHz
                        println!("✅ Frame reçue : {} échantillons", frame.samples.len());
                    },
                    Ok(Err(e)) => panic!("Erreur lors de la capture: {}", e),
                    Err(_) => panic!("Timeout - aucune frame reçue"),
                }
                
                let _ = capture.stop().await;
            }
        }
    }
}
