//! Configuration audio pour le système Voc
//! 
//! Ce module définit tous les paramètres audio utilisés par l'application.
//! Ces paramètres sont cruciaux pour la qualité et la latence de la communication vocale.

use serde::{Deserialize, Serialize};

/// Configuration principale pour tout le système audio
/// 
/// Cette structure contient tous les paramètres nécessaires pour configurer :
/// - La capture audio (microphone)  
/// - La compression Opus
/// - La lecture audio (haut-parleurs)
/// 
/// `#[derive(Clone)]` : Permet de dupliquer facilement cette config
/// `#[derive(Debug)]` : Permet d'afficher la config pour le débogage  
/// `#[derive(Serialize, Deserialize)]` : Permet de sauvegarder/charger depuis un fichier
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Fréquence d'échantillonnage en Hz (échantillons par seconde)
    /// 
    /// 48000 Hz = qualité professionnelle
    /// Plus c'est élevé, plus la qualité est bonne mais plus ça consomme de bande passante
    pub sample_rate: u32,
    
    /// Nombre de canaux audio
    /// 
    /// 1 = Mono (un seul canal)
    /// 2 = Stéréo (gauche + droite)
    /// Pour la voix, mono suffit largement
    pub channels: u16,
    
    /// Durée de chaque frame audio en millisecondes
    /// 
    /// 20ms = bon compromis entre latence et efficacité de compression
    /// Plus petit = moins de latence mais compression moins efficace
    /// Plus grand = meilleure compression mais plus de latence
    pub frame_duration_ms: u16,
    
    /// Débit cible pour la compression Opus en bits par seconde
    /// 
    /// 32000 bps = 32 kbps = qualité vocale excellente
    /// Opus peut descendre jusqu'à 6 kbps pour la voix
    pub opus_bitrate: u32,
    
    /// Complexité de l'encodeur Opus (0-10)
    /// 
    /// 0 = Très rapide, qualité moindre
    /// 10 = Très lent, meilleure qualité 
    /// 5 = Bon compromis pour temps réel
    pub opus_complexity: u32,
    
    /// Taille du buffer de réception en nombre de frames
    /// 
    /// Plus grand = plus de tolérance au jitter réseau
    /// Plus petit = moins de latence
    /// 3 frames = ~60ms de buffer
    pub receive_buffer_size: usize,
}

impl Default for AudioConfig {
    /// Configuration par défaut optimisée pour la communication vocale LAN
    fn default() -> Self {
        Self {
            sample_rate: 48000,         // 48 kHz - standard professionnel
            channels: 1,                // Mono pour la voix
            frame_duration_ms: 20,      // 20ms - standard VoIP
            opus_bitrate: 32000,        // 32 kbps - excellente qualité vocale
            opus_complexity: 5,         // Complexité moyenne
            receive_buffer_size: 3,     // 3 frames = 60ms buffer
        }
    }
}

impl AudioConfig {
    /// Calcule le nombre d'échantillons par frame
    /// 
    /// Formule : (sample_rate * frame_duration_ms) / 1000
    /// Exemple : (48000 * 20) / 1000 = 960 échantillons
    pub fn samples_per_frame(&self) -> usize {
        (self.sample_rate as f32 * self.frame_duration_ms as f32 / 1000.0) as usize
    }
    
    /// Calcule la taille en bytes d'une frame audio brute (non compressée)
    /// 
    /// Chaque échantillon = f32 = 4 bytes
    /// Taille = nombre_échantillons * channels * 4 bytes
    pub fn frame_size_bytes(&self) -> usize {
        self.samples_per_frame() * self.channels as usize * 4
    }
    
    /// Taille maximale estimée d'une frame compressée Opus
    /// 
    /// Opus peut théoriquement générer jusqu'à 4000 bytes pour 20ms
    /// En pratique, pour la voix, c'est plutôt 80-200 bytes
    pub fn max_compressed_frame_size(&self) -> usize {
        4000
    }
    
    /// Calcule la latence théorique minimale du système
    /// 
    /// Latence = durée_frame + buffer_size * durée_frame
    /// C'est le temps minimal entre la capture et la lecture
    pub fn theoretical_latency_ms(&self) -> u32 {
        self.frame_duration_ms as u32 * (1 + self.receive_buffer_size as u32)
    }
    
    /// Valide que la configuration est cohérente
    /// 
    /// Vérifie que tous les paramètres sont dans des plages acceptables
    pub fn validate(&self) -> Result<(), String> {
        if self.sample_rate < 8000 || self.sample_rate > 48000 {
            return Err(format!("Sample rate invalide: {} (doit être entre 8000 et 48000)", self.sample_rate));
        }
        
        if self.channels == 0 || self.channels > 2 {
            return Err(format!("Nombre de canaux invalide: {} (doit être 1 ou 2)", self.channels));
        }
        
        if self.frame_duration_ms < 10 || self.frame_duration_ms > 60 {
            return Err(format!("Durée de frame invalide: {}ms (doit être entre 10 et 60)", self.frame_duration_ms));
        }
        
        if self.opus_bitrate < 6000 || self.opus_bitrate > 128000 {
            return Err(format!("Bitrate Opus invalide: {} (doit être entre 6000 et 128000)", self.opus_bitrate));
        }
        
        if self.opus_complexity > 10 {
            return Err(format!("Complexité Opus invalide: {} (doit être entre 0 et 10)", self.opus_complexity));
        }
        
        Ok(())
    }
    
    /// Crée une configuration optimisée pour faible latence
    pub fn low_latency() -> Self {
        Self {
            frame_duration_ms: 10,      // Frames plus petites
            receive_buffer_size: 2,     // Buffer plus petit
            opus_complexity: 3,         // Moins de complexité CPU
            ..Default::default()
        }
    }
    
    /// Crée une configuration optimisée pour la qualité
    pub fn high_quality() -> Self {
        Self {
            opus_bitrate: 64000,        // Bitrate plus élevé
            opus_complexity: 8,         // Plus de complexité
            receive_buffer_size: 5,     // Buffer plus grand pour stabilité
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = AudioConfig::default();
        
        // Test des calculs
        assert_eq!(config.samples_per_frame(), 960); // 48000 * 20 / 1000
        assert_eq!(config.frame_size_bytes(), 3840); // 960 * 1 * 4
        assert_eq!(config.theoretical_latency_ms(), 80); // 20 * (1 + 3)
        
        // Test de validation
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_invalid_config() {
        let mut config = AudioConfig::default();
        
        config.sample_rate = 1000; // Trop bas
        assert!(config.validate().is_err());
        
        config.sample_rate = 48000;
        config.channels = 0; // Invalide
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_preset_configs() {
        let low_lat = AudioConfig::low_latency();
        assert_eq!(low_lat.frame_duration_ms, 10);
        assert!(low_lat.validate().is_ok());
        
        let high_qual = AudioConfig::high_quality();
        assert_eq!(high_qual.opus_bitrate, 64000);
        assert!(high_qual.validate().is_ok());
    }
}
