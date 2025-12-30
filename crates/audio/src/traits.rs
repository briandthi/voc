//! Traits abstraits pour le système audio
//! 
//! Ce module définit les interfaces (traits) que doivent implémenter
//! tous les composants audio. Cela permet d'avoir du code modulaire
//! et testable avec différentes implémentations.

use async_trait::async_trait;
use crate::{AudioFrame, CompressedFrame, AudioError, AudioResult};

/// Trait pour capturer l'audio depuis un périphérique d'entrée
/// 
/// Ce trait abstrait permet d'utiliser différentes implémentations :
/// - CpalCapture : Implémentation avec la librairie cpal
/// - MockCapture : Implémentation factice pour les tests
/// - FileCapture : Lecture depuis un fichier audio (pour debug)
/// 
/// `#[async_trait]` permet d'avoir des fonctions async dans les traits.
/// `Send` indique que l'objet peut être transféré entre threads.
#[async_trait]
pub trait AudioCapture: Send + Sync {
    /// Démarre la capture audio
    /// 
    /// Cette fonction initialise le périphérique et commence à capturer l'audio.
    /// Elle doit être appelée avant `next_frame()`.
    /// 
    /// # Erreurs
    /// - `AudioError::NoDeviceFound` : Aucun microphone trouvé
    /// - `AudioError::ConfigError` : Problème de configuration
    /// - `AudioError::InitializationError` : Échec de l'initialisation
    async fn start(&mut self) -> AudioResult<()>;
    
    /// Arrête la capture audio
    /// 
    /// Libère les ressources et ferme le périphérique.
    /// Après cet appel, `next_frame()` ne doit plus être utilisé.
    async fn stop(&mut self) -> AudioResult<()>;
    
    /// Récupère la prochaine frame audio
    /// 
    /// Cette fonction bloque jusqu'à ce qu'une frame soit disponible.
    /// Elle retourne une frame d'environ 20ms d'audio.
    /// 
    /// # Erreurs  
    /// - `AudioError::DeviceDisconnected` : Microphone débranché
    /// - `AudioError::Timeout` : Pas de données reçues dans le délai
    /// - `AudioError::BufferOverflow` : Trop de données en attente
    /// 
    /// # Example
    /// ```rust,no_run
    /// use audio::{AudioCapture, CpalCapture, AudioConfig};
    /// 
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = AudioConfig::default();
    /// let mut capture = CpalCapture::new(config)?;
    /// 
    /// capture.start().await?;
    /// 
    /// loop {
    ///     let frame = capture.next_frame().await?;
    ///     println!("Reçu frame avec {} échantillons", frame.samples.len());
    /// }
    /// # }
    /// ```
    async fn next_frame(&mut self) -> AudioResult<AudioFrame>;
    
    /// Vérifie si la capture est active
    /// 
    /// Retourne `true` si `start()` a été appelé et que la capture fonctionne.
    fn is_recording(&self) -> bool;
    
    /// Retourne des informations sur le périphérique utilisé
    /// 
    /// Utile pour l'interface utilisateur ou le debug.
    fn device_info(&self) -> String {
        "Périphérique inconnu".to_string()
    }
}

/// Trait pour jouer l'audio sur un périphérique de sortie
/// 
/// Ce trait gère la lecture des frames audio vers les haut-parleurs
/// ou casque. Il inclut un système de buffering pour gérer le jitter.
#[async_trait]
pub trait AudioPlayback: Send + Sync {
    /// Démarre la lecture audio
    /// 
    /// Initialise le périphérique de sortie et prépare les buffers.
    async fn start(&mut self) -> AudioResult<()>;
    
    /// Arrête la lecture audio
    /// 
    /// Vide les buffers et ferme le périphérique.
    async fn stop(&mut self) -> AudioResult<()>;
    
    /// Met une frame en queue pour lecture
    /// 
    /// La frame sera jouée dans l'ordre d'arrivée.
    /// Si le buffer est plein, la frame peut être rejetée.
    /// 
    /// # Arguments
    /// * `frame` - La frame audio à jouer
    /// 
    /// # Erreurs
    /// - `AudioError::BufferOverflow` : Buffer plein, frame rejetée
    /// - `AudioError::DeviceDisconnected` : Haut-parleurs débranchés
    /// 
    /// # Example
    /// ```rust,no_run
    /// use audio::{AudioPlayback, CpalPlayback, AudioFrame};
    /// 
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let config = audio::AudioConfig::default();
    /// let mut playback = CpalPlayback::new(config)?;
    /// 
    /// playback.start().await?;
    /// 
    /// let frame = AudioFrame::silence(960, 1);
    /// playback.play_frame(frame).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn play_frame(&mut self, frame: AudioFrame) -> AudioResult<()>;
    
    /// Vérifie si la lecture est active
    fn is_playing(&self) -> bool;
    
    /// Retourne le niveau du buffer (nombre de frames en attente)
    /// 
    /// Utile pour ajuster la latence et détecter les problèmes.
    /// - Valeur basse = risque d'underrun (coupures)
    /// - Valeur élevée = latence importante
    fn buffer_level(&self) -> usize;
    
    /// Vide le buffer de lecture
    /// 
    /// Utile pour récupérer d'un décrochage réseau.
    async fn flush_buffer(&mut self) -> AudioResult<()> {
        // Implémentation par défaut : rien à faire
        Ok(())
    }
    
    /// Retourne des informations sur le périphérique de sortie
    fn device_info(&self) -> String {
        "Périphérique de sortie inconnu".to_string()
    }
}

/// Trait pour encoder/décoder l'audio avec un codec
/// 
/// Ce trait abstrait la compression/décompression audio.
/// L'implémentation principale utilise Opus, mais on peut en imaginer d'autres.
pub trait AudioCodec: Send + Sync {
    /// Encode (compresse) une frame audio
    /// 
    /// Prend une frame audio brute et la compresse pour transmission.
    /// 
    /// # Arguments
    /// * `frame` - La frame audio à compresser
    /// 
    /// # Returns
    /// Une frame compressée prête à être envoyée sur le réseau
    /// 
    /// # Erreurs
    /// - `AudioError::OpusError` : Erreur du codec
    /// - `AudioError::ConfigError` : Paramètres invalides
    /// 
    /// # Example
    /// ```rust,no_run
    /// use audio::{AudioCodec, OpusCodec, AudioFrame, AudioConfig};
    /// 
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = AudioConfig::default();
    /// let mut codec = OpusCodec::new(config)?;
    /// 
    /// let frame = AudioFrame::silence(960, 1);
    /// let compressed = codec.encode(&frame)?;
    /// 
    /// println!("Frame compressée : {} bytes", compressed.data.len());
    /// # Ok(())
    /// # }
    /// ```
    fn encode(&mut self, frame: &AudioFrame) -> AudioResult<CompressedFrame>;
    
    /// Décode (décompresse) une frame audio
    /// 
    /// Prend une frame compressée et la décompresse pour lecture.
    /// 
    /// # Arguments  
    /// * `compressed` - La frame compressée à décoder
    /// 
    /// # Returns
    /// Une frame audio brute prête à être jouée
    /// 
    /// # Erreurs
    /// - `AudioError::OpusError` : Données corrompues ou codec défaillant
    /// 
    /// # Example
    /// ```rust,no_run
    /// # use audio::{AudioCodec, OpusCodec, CompressedFrame, AudioConfig};
    /// # use std::time::Instant;
    /// 
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let config = AudioConfig::default();
    /// # let mut codec = OpusCodec::new(config)?;
    /// 
    /// // Supposons qu'on a reçu une frame du réseau
    /// let compressed = CompressedFrame::new(
    ///     vec![1, 2, 3, 4], // Données Opus
    ///     960,              // Échantillons originaux
    ///     Instant::now(),
    ///     42
    /// );
    /// 
    /// let decoded = codec.decode(&compressed)?;
    /// println!("Frame décodée : {} échantillons", decoded.samples.len());
    /// # Ok(())
    /// # }
    /// ```
    fn decode(&mut self, compressed: &CompressedFrame) -> AudioResult<AudioFrame>;
    
    /// Réinitialise l'état interne du codec
    /// 
    /// Utile après une coupure réseau ou pour débuter une nouvelle session.
    /// Les codecs ont souvent un état interne (prédictions, etc.).
    fn reset(&mut self) -> AudioResult<()>;
    
    /// Retourne des informations sur la configuration du codec
    fn codec_info(&self) -> String {
        "Codec audio".to_string()
    }
}

/// Trait pour un pipeline audio complet
/// 
/// Ce trait combine capture, codec et playback pour des tests end-to-end.
/// Il permet de tester tout le système audio sans réseau.
#[async_trait]
pub trait AudioPipeline: Send + Sync {
    /// Démarre le pipeline complet
    /// 
    /// Initialise capture, codec et playback.
    async fn start(&mut self) -> AudioResult<()>;
    
    /// Arrête le pipeline complet
    /// 
    /// Ferme tous les composants.
    async fn stop(&mut self) -> AudioResult<()>;
    
    /// Lance un test de bouclage (loopback)
    /// 
    /// Capture audio → encode → decode → lecture en boucle.
    /// Utile pour tester la latence et la qualité.
    /// 
    /// # Arguments
    /// * `duration_seconds` - Durée du test
    /// 
    /// # Returns
    /// Statistiques sur le test effectué
    async fn run_loopback_test(&mut self, duration_seconds: u32) -> AudioResult<crate::AudioStats>;
    
    /// Traite une frame complète (capture → encode → decode → lecture)
    /// 
    /// Fonction de test pour valider une frame isolée.
    async fn process_single_frame(&mut self) -> AudioResult<()>;
}

/// Trait pour surveiller les performances audio
/// 
/// Permet de collecter des métriques sur le système audio.
pub trait AudioMonitor: Send + Sync {
    /// Met à jour les statistiques avec une nouvelle frame
    fn record_frame_captured(&mut self, frame: &AudioFrame);
    fn record_frame_played(&mut self, frame: &AudioFrame);
    fn record_frame_lost(&mut self, sequence_number: u64);
    fn record_latency(&mut self, latency_ms: f32);
    fn record_compression_ratio(&mut self, ratio: f32);
    
    /// Récupère les statistiques actuelles
    fn get_stats(&self) -> crate::AudioStats;
    
    /// Remet les statistiques à zéro
    fn reset_stats(&mut self);
}

/// Trait pour les dispositifs audio factices (tests)
/// 
/// Permet de créer des implémentations de test qui simulent
/// des périphériques audio sans avoir besoin de hardware.
pub trait MockAudioDevice: Send + Sync {
    /// Configure le dispositif factice avec des données de test
    fn set_test_data(&mut self, frames: Vec<AudioFrame>);
    
    /// Simule une erreur de périphérique
    fn simulate_error(&mut self, error: AudioError);
    
    /// Active/désactive la simulation de latence
    fn set_simulated_latency(&mut self, latency_ms: u32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    
    // Test qu'on peut créer des objets trait
    // (ne teste pas les implémentations, juste les interfaces)
    
    #[test]
    fn test_audio_frame_creation() {
        let frame = AudioFrame::new(vec![0.0, 0.1, -0.1], 1);
        assert_eq!(frame.samples.len(), 3);
        assert_eq!(frame.sequence_number, 1);
    }
    
    #[test]
    fn test_compressed_frame_creation() {
        let compressed = CompressedFrame::new(
            vec![1, 2, 3],
            960,
            Instant::now(),
            42
        );
        assert_eq!(compressed.data.len(), 3);
        assert_eq!(compressed.original_sample_count, 960);
        assert_eq!(compressed.sequence_number, 42);
    }
}
