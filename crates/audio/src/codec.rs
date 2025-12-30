//! Module de compression/décompression audio avec Opus
//! 
//! Ce module implémente le trait AudioCodec en utilisant la librairie Opus.
//! Opus est un codec audio open-source optimisé pour la communication vocale
//! et la musique, avec une excellente qualité à bas débit.
//!
//! Opus est particulièrement adapté pour VoIP car il :
//! - Supporte des débits très bas (6-128 kbps)
//! - A une latence très faible (2.5-60ms)
//! - S'adapte automatiquement au contenu (voix vs musique)
//! - Résiste bien aux pertes de paquets réseau

use opus::{Encoder, Decoder, Application, Channels};
use std::sync::Mutex;

use crate::{
    AudioCodec, AudioFrame, CompressedFrame, AudioConfig, AudioError, AudioResult,
};

/// Implémentation du codec Opus avec thread safety
/// 
/// Cette structure gère un encodeur et un décodeur Opus configurés
/// pour la communication vocale temps réel. Les codecs sont protégés
/// par un Mutex pour assurer la thread safety requise par le trait AudioCodec.
/// 
/// # Architecture Opus
/// 
/// Opus combine deux technologies :
/// - SILK : Optimisé pour la voix (débits bas)
/// - CELT : Optimisé pour la musique (faible latence)
/// 
/// Il choisit automatiquement le meilleur algorithme selon le contenu.
/// 
/// # Thread Safety
/// 
/// Opus lui-même n'est pas thread-safe au niveau d'une instance,
/// mais c'est sûr d'avoir différentes instances sur différents threads.
/// Nous utilisons un Mutex pour protéger l'accès aux codecs et garantir
/// qu'un seul thread à la fois peut encoder/décoder.
pub struct OpusCodec {
    /// Structure interne protégée par Mutex pour thread safety
    inner: Mutex<OpusCodecInner>,
}

/// Structure interne contenant les vrais codecs Opus
struct OpusCodecInner {
    /// Encodeur Opus pour compresser l'audio
    encoder: Encoder,
    
    /// Décodeur Opus pour décompresser l'audio
    decoder: Decoder,
    
    /// Configuration audio utilisée
    config: AudioConfig,
    
    /// Buffer pour les données compressées
    compressed_buffer: Vec<u8>,
    
    /// Buffer pour les données décompressées  
    decompressed_buffer: Vec<f32>,
}

impl OpusCodec {
    /// Crée un nouveau codec Opus
    /// 
    /// Cette fonction initialise l'encodeur et le décodeur avec les paramètres
    /// optimaux pour la communication vocale.
    /// 
    /// # Arguments
    /// * `config` - Configuration audio à utiliser
    /// 
    /// # Erreurs
    /// - `AudioError::OpusError` si l'initialisation échoue
    /// - `AudioError::ConfigError` si la configuration n'est pas supportée
    pub fn new(config: AudioConfig) -> AudioResult<Self> {
        // Valide la configuration avant de créer le codec
        config.validate()
            .map_err(|e| AudioError::ConfigError(e))?;
        
        println!("🎵 Initialisation codec Opus :");
        println!("   Sample rate : {} Hz", config.sample_rate);
        println!("   Channels : {}", config.channels);
        println!("   Bitrate : {} bps", config.opus_bitrate);
        println!("   Complexité : {}", config.opus_complexity);
        
        // Convertit notre configuration vers le format Opus
        let opus_channels = match config.channels {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            _ => return Err(AudioError::ConfigError(format!(
                "Nombre de canaux non supporté par Opus: {}", config.channels
            ))),
        };
        
        // Crée l'encodeur Opus
        // Application::Voip optimise pour la voix avec suppression d'écho
        let mut encoder = Encoder::new(
            config.sample_rate,
            opus_channels,
            Application::Voip, // Optimisé pour VoIP
        ).map_err(|e| AudioError::OpusError(format!("Impossible de créer l'encodeur: {:?}", e)))?;
        
        // Configure l'encodeur
        encoder.set_bitrate(opus::Bitrate::Bits(config.opus_bitrate as i32))
            .map_err(|e| AudioError::OpusError(format!("Impossible de définir le bitrate: {:?}", e)))?;
        
        // Note: set_complexity n'est pas disponible dans cette version d'Opus
        // La complexité est gérée automatiquement
        
        // Note: set_signal n'est pas disponible dans cette version d'Opus
        // Le codec s'adapte automatiquement au contenu
        
        // Active l'adaptation automatique du débit
        encoder.set_vbr(true)
            .map_err(|e| AudioError::OpusError(format!("Impossible d'activer VBR: {:?}", e)))?;
        
        // Crée le décodeur Opus
        let decoder = Decoder::new(
            config.sample_rate,
            opus_channels,
        ).map_err(|e| AudioError::OpusError(format!("Impossible de créer le décodeur: {:?}", e)))?;
        
        // Prépare les buffers de travail
        let max_compressed_size = config.max_compressed_frame_size();
        let max_samples = config.samples_per_frame() * config.channels as usize;
        
        println!("✅ Codec Opus initialisé");
        println!("   Taille buffer compressé : {} bytes", max_compressed_size);
        println!("   Taille buffer décompressé : {} échantillons", max_samples);
        
        let inner = OpusCodecInner {
            encoder,
            decoder,
            config,
            compressed_buffer: vec![0u8; max_compressed_size],
            decompressed_buffer: vec![0.0f32; max_samples],
        };

        Ok(Self {
            inner: Mutex::new(inner),
        })
    }
    
    /// Retourne des informations détaillées sur la configuration du codec
    pub fn detailed_info(&self) -> String {
        let inner = self.inner.lock().unwrap();
        format!(
            "Opus Codec - {}Hz, {} ch, {}bps, complexité {}",
            inner.config.sample_rate,
            inner.config.channels,
            inner.config.opus_bitrate,
            inner.config.opus_complexity
        )
    }
    
    /// Teste le codec avec une frame de silence
    /// 
    /// Utile pour vérifier que tout fonctionne correctement
    pub fn test_codec(&mut self) -> AudioResult<()> {
        println!("🧪 Test du codec Opus...");
        
        // Crée une frame de test (silence)
        let samples_per_frame = {
            let inner = self.inner.lock().unwrap();
            inner.config.samples_per_frame()
        };
        let test_frame = AudioFrame::silence(samples_per_frame, 0);
        
        // Test encode
        let compressed = self.encode(&test_frame)?;
        println!("   Compression : {} → {} bytes (ratio: {:.1}x)", 
                test_frame.samples.len() * 4, 
                compressed.data.len(),
                compressed.compression_ratio());
        
        // Test decode
        let decoded = self.decode(&compressed)?;
        println!("   Décompression : {} → {} échantillons", 
                compressed.data.len(),
                decoded.samples.len());
        
        // Vérifie la cohérence
        if decoded.samples.len() != test_frame.samples.len() {
            return Err(AudioError::OpusError(format!(
                "Incohérence taille : {} → {}", 
                test_frame.samples.len(), 
                decoded.samples.len()
            )));
        }
        
        println!("✅ Test codec réussi");
        Ok(())
    }
}

impl AudioCodec for OpusCodec {
    fn encode(&mut self, frame: &AudioFrame) -> AudioResult<CompressedFrame> {
        let mut inner = self.inner.lock().unwrap();
        
        // Vérifie que la frame a la bonne taille
        let expected_samples = inner.config.samples_per_frame() * inner.config.channels as usize;
        if frame.samples.len() != expected_samples {
            return Err(AudioError::OpusError(format!(
                "Taille de frame incorrecte: {} échantillons (attendu: {})",
                frame.samples.len(),
                expected_samples
            )));
        }
        
        // Encode la frame avec Opus
        // Nous devons séparer l'accès à l'encoder et au buffer pour satisfaire le borrow checker
        let encoded_size = {
            let OpusCodecInner { encoder, compressed_buffer, .. } = &mut *inner;
            encoder.encode_float(
                &frame.samples,
                compressed_buffer
            ).map_err(|e| AudioError::OpusError(format!("Erreur encodage: {:?}", e)))?
        };
        
        // Crée la frame compressée
        let compressed_data = inner.compressed_buffer[..encoded_size].to_vec();
        
        Ok(CompressedFrame::new(
            compressed_data,
            frame.samples.len(),
            frame.timestamp,
            frame.sequence_number,
        ))
    }
    
    fn decode(&mut self, compressed: &CompressedFrame) -> AudioResult<AudioFrame> {
        let mut inner = self.inner.lock().unwrap();
        
        // Redimensionne le buffer si nécessaire
        let expected_samples = compressed.original_sample_count;
        if inner.decompressed_buffer.len() < expected_samples {
            inner.decompressed_buffer.resize(expected_samples, 0.0);
        }
        
        // Décode avec Opus
        // Utilisation de destructuring pour éviter les conflits de borrow
        let decoded_samples = {
            let OpusCodecInner { decoder, decompressed_buffer, .. } = &mut *inner;
            decoder.decode_float(
                &compressed.data,
                &mut decompressed_buffer[..expected_samples],
                false // fec (forward error correction) désactivé pour l'instant
            ).map_err(|e| AudioError::OpusError(format!("Erreur décodage Opus: {:?}", e)))?
        };
        
        // Vérifie que le décodage a produit le bon nombre d'échantillons
        if decoded_samples != expected_samples {
            return Err(AudioError::OpusError(format!(
                "Décodage incohérent: {} échantillons décodés (attendu: {})",
                decoded_samples,
                expected_samples
            )));
        }
        
        // Crée la frame décodée
        Ok(AudioFrame::new(
            inner.decompressed_buffer[..decoded_samples].to_vec(),
            compressed.sequence_number,
        ))
    }
    
    fn reset(&mut self) -> AudioResult<()> {
        let mut inner = self.inner.lock().unwrap();
        
        // Reset l'encodeur
        inner.encoder.reset_state()
            .map_err(|e| AudioError::OpusError(format!("Impossible de réinitialiser l'encodeur: {:?}", e)))?;
        
        // Reset le décodeur  
        inner.decoder.reset_state()
            .map_err(|e| AudioError::OpusError(format!("Impossible de réinitialiser le décodeur: {:?}", e)))?;
        
        println!("🔄 Codec Opus réinitialisé");
        Ok(())
    }
    
    fn codec_info(&self) -> String {
        self.detailed_info()
    }
}

// Implémentation de Drop pour nettoyer proprement
impl Drop for OpusCodec {
    fn drop(&mut self) {
        println!("🧹 Nettoyage du codec Opus");
        // Les structures Opus se nettoient automatiquement
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_opus_codec_creation() {
        let config = AudioConfig::default();
        
        match OpusCodec::new(config) {
            Ok(codec) => {
                assert!(codec.codec_info().contains("Opus"));
                println!("✅ Codec créé: {}", codec.codec_info());
            },
            Err(e) => panic!("Impossible de créer le codec Opus: {}", e),
        }
    }
    
    #[test]
    fn test_opus_encode_decode() {
        let config = AudioConfig::default();
        let mut codec = OpusCodec::new(config.clone()).expect("Création codec");
        
        // Test avec du silence
        let silence_frame = AudioFrame::silence(config.samples_per_frame(), 42);
        
        // Encode
        let compressed = codec.encode(&silence_frame).expect("Encodage");
        assert!(compressed.data.len() > 0);
        assert!(compressed.data.len() < silence_frame.samples.len() * 4); // Doit être compressé
        assert_eq!(compressed.sequence_number, 42);
        
        // Decode
        let decoded = codec.decode(&compressed).expect("Décodage");
        assert_eq!(decoded.samples.len(), silence_frame.samples.len());
        assert_eq!(decoded.sequence_number, 42);
        
        // Pour le silence, on s'attend à des valeurs très proches de 0
        let max_silence_error = decoded.samples.iter()
            .map(|&s| s.abs())
            .fold(0.0, f32::max);
        assert!(max_silence_error < 0.1, "Erreur de silence trop importante: {}", max_silence_error);
        
        println!("✅ Test encode/decode silence réussi");
        println!("   Compression: {} → {} bytes (ratio: {:.1}x)", 
                silence_frame.samples.len() * 4, 
                compressed.data.len(),
                compressed.compression_ratio());
    }
    
    #[test]
    fn test_opus_sine_wave() {
        let config = AudioConfig::default();
        let mut codec = OpusCodec::new(config.clone()).expect("Création codec");
        
        // Génère une onde sinusoïdale de test (440 Hz = La)
        let samples_per_frame = config.samples_per_frame();
        let sample_rate = config.sample_rate as f32;
        let frequency = 440.0; // Hz
        
        let mut sine_samples = Vec::with_capacity(samples_per_frame);
        for i in 0..samples_per_frame {
            let t = i as f32 / sample_rate;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5; // Amplitude 0.5
            sine_samples.push(sample);
        }
        
        let sine_frame = AudioFrame::new(sine_samples.clone(), 1);
        
        // Encode/Decode
        let compressed = codec.encode(&sine_frame).expect("Encodage onde");
        let decoded = codec.decode(&compressed).expect("Décodage onde");
        
        assert_eq!(decoded.samples.len(), sine_frame.samples.len());
        
        // Calcule l'erreur RMS entre original et décodé
        let mut sum_error_squared = 0.0;
        for (orig, decoded) in sine_samples.iter().zip(decoded.samples.iter()) {
            let error = orig - decoded;
            sum_error_squared += error * error;
        }
        let rms_error = (sum_error_squared / sine_samples.len() as f32).sqrt();
        
        println!("✅ Test encode/decode onde sinusoïdale réussi");
        println!("   Compression: {} → {} bytes (ratio: {:.1}x)", 
                sine_frame.samples.len() * 4, 
                compressed.data.len(),
                compressed.compression_ratio());
        println!("   Erreur RMS: {:.6}", rms_error);
        
        // Pour une onde simple, Opus devrait avoir une erreur très faible
        assert!(rms_error < 0.05, "Erreur RMS trop importante: {}", rms_error);
    }
    
    #[test]
    fn test_opus_codec_reset() {
        let config = AudioConfig::default();
        let mut codec = OpusCodec::new(config).expect("Création codec");
        
        // Test que reset ne cause pas d'erreur
        codec.reset().expect("Reset codec");
        
        println!("✅ Test reset codec réussi");
    }
    
    #[test]
    fn test_opus_invalid_frame_size() {
        let config = AudioConfig::default();
        let mut codec = OpusCodec::new(config).expect("Création codec");
        
        // Frame avec mauvaise taille
        let bad_frame = AudioFrame::new(vec![0.0; 100], 1); // Taille incorrecte
        
        // L'encodage doit échouer
        match codec.encode(&bad_frame) {
            Err(AudioError::OpusError(_)) => {
                println!("✅ Erreur de taille correctement détectée");
            },
            Ok(_) => panic!("L'encodage aurait dû échouer"),
            Err(e) => panic!("Type d'erreur inattendu: {}", e),
        }
    }
}
