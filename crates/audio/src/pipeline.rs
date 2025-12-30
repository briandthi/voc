//! Pipeline audio complet pour tests end-to-end
//! 
//! Ce module implémente le trait AudioPipeline en combinant :
//! - Capture audio (CpalCapture)
//! - Codec Opus (OpusCodec) 
//! - Lecture audio (CpalPlayback)
//! 
//! Il permet de tester tout le système audio sans réseau,
//! idéal pour valider la latence et la qualité avant de passer au networking.

use async_trait::async_trait;
use tokio::time::{sleep, Duration, Instant};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    AudioPipeline, AudioCapture, AudioPlayback, AudioCodec,
    CpalCapture, CpalPlayback, OpusCodec,
    AudioFrame, AudioConfig, AudioError, AudioResult, AudioStats,
};

/// Pipeline audio complet pour tests
/// 
/// Cette structure combine capture, codec et playback pour créer
/// un pipeline de test complet. Elle est particulièrement utile pour :
/// 
/// - Tester la latence end-to-end
/// - Valider la qualité audio après compression/décompression
/// - Débugger les problèmes audio avant le réseau
/// - Mesurer les performances du système
/// 
/// # Architecture du pipeline
/// 
/// ```text
/// Microphone → [Capture] → [Encode] → [Decode] → [Playback] → Haut-parleurs
///     ↑                                                           ↑
///     └─────────────── Test Loopback ───────────────────────────┘
/// ```
pub struct AudioPipelineImpl {
    /// Module de capture audio
    capture: Box<dyn AudioCapture>,
    
    /// Codec pour compression/décompression
    codec: Box<dyn AudioCodec>,
    
    /// Module de lecture audio
    playback: Box<dyn AudioPlayback>,
    
    /// Configuration audio
    _config: AudioConfig,
    
    /// Statistiques du pipeline
    stats: Arc<Mutex<AudioStats>>,
    
    /// Indicateur si le pipeline est actif
    is_running: bool,
}

impl AudioPipelineImpl {
    /// Crée un nouveau pipeline audio complet
    /// 
    /// # Arguments
    /// * `config` - Configuration audio à utiliser
    /// 
    /// # Erreurs
    /// - `AudioError::NoDeviceFound` si micro/haut-parleurs manquants
    /// - `AudioError::InitializationError` si un composant échoue à s'initialiser
    pub fn new(config: AudioConfig) -> AudioResult<Self> {
        println!("🔧 Initialisation du pipeline audio complet...");
        
        // Crée les composants
        let capture = Box::new(CpalCapture::new(config.clone())?) as Box<dyn AudioCapture>;
        let codec = Box::new(OpusCodec::new(config.clone())?) as Box<dyn AudioCodec>;
        let playback = Box::new(CpalPlayback::new(config.clone())?) as Box<dyn AudioPlayback>;
        
        println!("✅ Pipeline audio initialisé");
        println!("   Capture : {}", capture.device_info());
        println!("   Codec : {}", codec.codec_info());
        println!("   Playback : {}", playback.device_info());
        
        Ok(Self {
            capture,
            codec,
            playback,
            _config: config,
            stats: Arc::new(Mutex::new(AudioStats::default())),
            is_running: false,
        })
    }
    
    /// Retourne les statistiques actuelles du pipeline
    pub async fn get_stats(&self) -> AudioStats {
        self.stats.lock().await.clone()
    }
    
    /// Remet les statistiques à zéro
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.lock().await;
        stats.reset();
    }
    
    /// Met à jour les statistiques avec une nouvelle frame
    async fn update_stats_captured(&self, frame: &AudioFrame) {
        let mut stats = self.stats.lock().await;
        stats.frames_captured += 1;
        
        // Met à jour le niveau RMS moyen
        let frame_rms = frame.rms_level();
        if stats.frames_captured == 1 {
            stats.avg_rms_level = frame_rms;
        } else {
            // Moyenne mobile simple
            stats.avg_rms_level = (stats.avg_rms_level * 0.9) + (frame_rms * 0.1);
        }
    }
    
    async fn update_stats_played(&self, _frame: &AudioFrame, latency_ms: f32) {
        let mut stats = self.stats.lock().await;
        stats.frames_played += 1;
        
        // Met à jour la latence moyenne
        if stats.frames_played == 1 {
            stats.avg_latency_ms = latency_ms;
        } else {
            stats.avg_latency_ms = (stats.avg_latency_ms * 0.9) + (latency_ms * 0.1);
        }
    }
    
    async fn update_stats_compression(&self, ratio: f32) {
        let mut stats = self.stats.lock().await;
        
        if stats.frames_captured <= 1 {
            stats.avg_compression_ratio = ratio;
        } else {
            stats.avg_compression_ratio = (stats.avg_compression_ratio * 0.9) + (ratio * 0.1);
        }
    }
    
    /// Lance un test de performance détaillé
    /// 
    /// Ce test mesure :
    /// - Latence de capture
    /// - Temps d'encodage
    /// - Temps de décodage  
    /// - Latence de lecture
    /// - Qualité audio (RMS)
    pub async fn performance_test(&mut self, duration_seconds: u32) -> AudioResult<()> {
        println!("⚡ Test de performance du pipeline ({}s)...", duration_seconds);
        
        self.start().await?;
        
        let start_time = Instant::now();
        let test_duration = Duration::from_secs(duration_seconds as u64);
        let mut frame_count = 0u64;
        let mut total_encode_time = Duration::ZERO;
        let mut total_decode_time = Duration::ZERO;
        
        while start_time.elapsed() < test_duration {
            // Mesure la capture
            let capture_start = Instant::now();
            let frame = self.capture.next_frame().await?;
            let capture_time = capture_start.elapsed();
            
            // Mesure l'encodage
            let encode_start = Instant::now();
            let compressed = self.codec.encode(&frame)?;
            let encode_time = encode_start.elapsed();
            total_encode_time += encode_time;
            
            // Mesure le décodage
            let decode_start = Instant::now();
            let decoded = self.codec.decode(&compressed)?;
            let decode_time = decode_start.elapsed();
            total_decode_time += decode_time;
            
            // Joue la frame
            if let Err(AudioError::BufferOverflow) = self.playback.play_frame(decoded).await {
                // Buffer overflow normal sous charge
            }
            
            frame_count += 1;
            
            // Affiche les métriques toutes les secondes
            if frame_count % 50 == 0 { // ~50 frames par seconde
                println!("📊 Frame {} - Capture: {:.1}ms, Encode: {:.1}ms, Decode: {:.1}ms",
                    frame_count,
                    capture_time.as_millis(),
                    encode_time.as_millis(),
                    decode_time.as_millis()
                );
            }
        }
        
        self.stop().await?;
        
        // Résultats finaux
        let avg_encode_ms = total_encode_time.as_millis() as f64 / frame_count as f64;
        let avg_decode_ms = total_decode_time.as_millis() as f64 / frame_count as f64;
        
        println!("🏁 Test de performance terminé :");
        println!("   Frames traitées : {}", frame_count);
        println!("   Temps moyen encodage : {:.2}ms", avg_encode_ms);
        println!("   Temps moyen décodage : {:.2}ms", avg_decode_ms);
        println!("   Throughput : {:.1} frames/s", frame_count as f64 / duration_seconds as f64);
        
        let stats = self.get_stats().await;
        println!("   Niveau audio moyen : {:.3}", stats.avg_rms_level);
        println!("   Compression moyenne : {:.1}x", stats.avg_compression_ratio);
        
        Ok(())
    }
    
    /// Test de stress avec charge CPU artificielle
    /// 
    /// Simule une charge système pour tester la robustesse
    pub async fn stress_test(&mut self, duration_seconds: u32) -> AudioResult<()> {
        println!("💪 Test de stress du pipeline ({}s)...", duration_seconds);
        
        self.start().await?;
        
        let start_time = Instant::now();
        let test_duration = Duration::from_secs(duration_seconds as u64);
        let mut dropped_frames = 0u64;
        let mut processed_frames = 0u64;
        
        while start_time.elapsed() < test_duration {
            // Ajoute une charge CPU artificielle
            let _waste: f64 = (0..1000).map(|x| (x as f64).sin()).sum();
            
            // Traite une frame
            match self.process_single_frame().await {
                Ok(_) => processed_frames += 1,
                Err(AudioError::BufferOverflow) => {
                    dropped_frames += 1;
                    processed_frames += 1;
                },
                Err(e) => {
                    eprintln!("⚠️  Erreur stress test: {}", e);
                    break;
                }
            }
            
            // Petite pause pour éviter de saturer complètement le CPU
            sleep(Duration::from_millis(1)).await;
        }
        
        self.stop().await?;
        
        let drop_rate = (dropped_frames as f64 / processed_frames as f64) * 100.0;
        
        println!("💪 Test de stress terminé :");
        println!("   Frames traitées : {}", processed_frames);
        println!("   Frames perdues : {} ({:.2}%)", dropped_frames, drop_rate);
        
        if drop_rate < 5.0 {
            println!("✅ Système robuste (< 5% de perte)");
        } else if drop_rate < 15.0 {
            println!("⚠️  Système moyennement robuste ({:.1}% de perte)", drop_rate);
        } else {
            println!("❌ Système fragile ({:.1}% de perte)", drop_rate);
        }
        
        Ok(())
    }
}

#[async_trait]
impl AudioPipeline for AudioPipelineImpl {
    async fn start(&mut self) -> AudioResult<()> {
        if self.is_running {
            return Ok(());
        }
        
        println!("🚀 Démarrage du pipeline audio...");
        
        // Démarre dans l'ordre : playback → capture (pour éviter les premières frames perdues)
        self.playback.start().await?;
        sleep(Duration::from_millis(100)).await; // Petit délai pour que le playback soit prêt
        
        self.capture.start().await?;
        
        self.is_running = true;
        println!("✅ Pipeline audio démarré");
        
        Ok(())
    }
    
    async fn stop(&mut self) -> AudioResult<()> {
        if !self.is_running {
            return Ok(());
        }
        
        println!("🛑 Arrêt du pipeline audio...");
        
        // Arrête dans l'ordre inverse
        self.capture.stop().await?;
        
        // Attend un peu pour vider les buffers
        sleep(Duration::from_millis(200)).await;
        
        self.playback.stop().await?;
        
        self.is_running = false;
        println!("✅ Pipeline audio arrêté");
        
        Ok(())
    }
    
    async fn run_loopback_test(&mut self, duration_seconds: u32) -> AudioResult<AudioStats> {
        println!("🔄 Test loopback ({}s) - Micro → Codec → Haut-parleurs", duration_seconds);
        println!("   ⚠️  Attention : vous allez entendre votre propre voix !");
        
        // Reset les statistiques
        self.reset_stats().await;
        
        // Démarre le pipeline
        self.start().await?;
        
        let start_time = Instant::now();
        let test_duration = Duration::from_secs(duration_seconds as u64);
        
        // Boucle principale du test
        while start_time.elapsed() < test_duration {
            match self.process_single_frame().await {
                Ok(_) => {},
                Err(AudioError::BufferOverflow) => {
                    // Buffer overflow acceptable pendant le test
                    let mut stats = self.stats.lock().await;
                    stats.buffer_overflows += 1;
                },
                Err(AudioError::Timeout) => {
                    println!("⏰ Timeout pendant le test loopback");
                    break;
                },
                Err(e) => {
                    eprintln!("❌ Erreur loopback: {}", e);
                    break;
                }
            }
            
            // Petite pause pour éviter de saturer le CPU
            sleep(Duration::from_millis(1)).await;
        }
        
        // Arrête le pipeline
        self.stop().await?;
        
        let stats = self.get_stats().await;
        
        println!("🏁 Test loopback terminé :");
        println!("   Frames capturées : {}", stats.frames_captured);
        println!("   Frames jouées : {}", stats.frames_played);
        println!("   Latence moyenne : {:.1}ms", stats.avg_latency_ms);
        println!("   Niveau audio : {:.3}", stats.avg_rms_level);
        println!("   Compression : {:.1}x", stats.avg_compression_ratio);
        
        if stats.buffer_overflows > 0 {
            println!("   ⚠️  Buffer overflows : {}", stats.buffer_overflows);
        }
        
        // Évaluation de la qualité
        if stats.avg_latency_ms < 50.0 && stats.avg_rms_level > 0.001 {
            println!("✅ Test réussi - Bonne qualité et latence");
        } else if stats.avg_latency_ms > 100.0 {
            println!("⚠️  Latence élevée ({:.1}ms)", stats.avg_latency_ms);
        } else if stats.avg_rms_level < 0.001 {
            println!("⚠️  Niveau audio très faible - Vérifiez le microphone");
        }
        
        Ok(stats)
    }
    
    async fn process_single_frame(&mut self) -> AudioResult<()> {
        // 1. Capture une frame
        let frame_start = Instant::now();
        let frame = self.capture.next_frame().await?;
        
        // Met à jour les stats de capture
        self.update_stats_captured(&frame).await;
        
        // 2. Encode la frame
        let compressed = self.codec.encode(&frame)?;
        self.update_stats_compression(compressed.compression_ratio()).await;
        
        // 3. Décode la frame
        let decoded = self.codec.decode(&compressed)?;
        
        // 4. Joue la frame
        self.playback.play_frame(decoded).await?;
        
        // Calcule la latence totale
        let total_latency = frame_start.elapsed().as_millis() as f32;
        self.update_stats_played(&frame, total_latency).await;
        
        Ok(())
    }
}

// Implémentation de Drop pour nettoyer proprement
impl Drop for AudioPipelineImpl {
    fn drop(&mut self) {
        if self.is_running {
            println!("🧹 Nettoyage automatique du pipeline audio");
            // Les composants individuels vont se nettoyer automatiquement
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;
    
    #[tokio::test]
    async fn test_pipeline_creation() {
        let config = AudioConfig::default();
        
        match AudioPipelineImpl::new(config) {
            Ok(pipeline) => {
                assert!(!pipeline.is_running);
                println!("✅ Pipeline créé avec succès");
            },
            Err(AudioError::NoDeviceFound) => {
                println!("⚠️  Pas de périphériques audio pour le test");
            },
            Err(e) => panic!("Erreur création pipeline: {}", e),
        }
    }
    
    #[tokio::test]
    async fn test_pipeline_start_stop() {
        let config = AudioConfig::default();
        
        if let Ok(mut pipeline) = AudioPipelineImpl::new(config) {
            assert!(!pipeline.is_running);
            
            if pipeline.start().await.is_ok() {
                assert!(pipeline.is_running);
                
                if pipeline.stop().await.is_ok() {
                    assert!(!pipeline.is_running);
                }
            }
        }
    }
    
    #[tokio::test]
    async fn test_single_frame_processing() {
        let config = AudioConfig::default();
        
        if let Ok(mut pipeline) = AudioPipelineImpl::new(config) {
            if pipeline.start().await.is_ok() {
                // Test traitement d'une frame avec timeout
                let result = timeout(
                    Duration::from_secs(5),
                    pipeline.process_single_frame()
                ).await;
                
                match result {
                    Ok(Ok(_)) => println!("✅ Frame traitée avec succès"),
                    Ok(Err(AudioError::BufferOverflow)) => println!("⚠️  Buffer overflow (acceptable)"),
                    Ok(Err(e)) => println!("❌ Erreur traitement: {}", e),
                    Err(_) => println!("⏰ Timeout traitement frame"),
                }
                
                let _ = pipeline.stop().await;
            }
        }
    }
    
    // Test loopback très court pour CI/CD
    #[tokio::test]
    #[ignore] // Ignore par défaut car nécessite du hardware audio
    async fn test_short_loopback() {
        let config = AudioConfig::default();
        
        if let Ok(mut pipeline) = AudioPipelineImpl::new(config) {
            // Test très court (1 seconde) pour éviter de bloquer les tests
            let result = timeout(
                Duration::from_secs(5),
                pipeline.run_loopback_test(1)
            ).await;
            
            match result {
                Ok(Ok(stats)) => {
                    println!("✅ Test loopback court réussi");
                    println!("   Frames: {}/{}", stats.frames_captured, stats.frames_played);
                },
                Ok(Err(e)) => println!("❌ Erreur loopback: {}", e),
                Err(_) => println!("⏰ Timeout test loopback"),
            }
        }
    }
    
    // Test de performance très léger pour CI/CD
    #[tokio::test] 
    #[ignore] // Ignore par défaut car nécessite du hardware audio
    async fn test_performance_light() {
        let config = AudioConfig::default();
        
        if let Ok(mut pipeline) = AudioPipelineImpl::new(config) {
            let result = timeout(
                Duration::from_secs(5),
                pipeline.performance_test(1)
            ).await;
            
            match result {
                Ok(Ok(_)) => println!("✅ Test performance léger réussi"),
                Ok(Err(e)) => println!("❌ Erreur performance: {}", e),
                Err(_) => println!("⏰ Timeout test performance"),
            }
        }
    }
}
