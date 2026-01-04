//! Types de données pour le système networking
//! 
//! Ce module définit les structures principales pour la communication réseau :
//! - NetworkPacket : Paquet réseau pour transport audio P2P
//! - ConnectionState : États de connexion entre pairs
//! - NetworkConfig : Configuration du système réseau
//! - NetworkStats : Statistiques et métriques réseau

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use audio::CompressedFrame;

/// Paquet réseau pour le transport d'audio P2P
/// 
/// Cette structure encapsule les frames audio compressées pour transmission UDP.
/// Elle inclut les métadonnées nécessaires pour la détection d'erreurs,
/// la synchronisation et les statistiques de performance.
/// 
/// Structure du paquet :
/// - Header : métadonnées (32 bytes)
/// - Payload : frame audio compressée (80-200 bytes typique)
/// - Total : ~120-250 bytes par paquet (largement < MTU 1400 bytes)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkPacket {
    /// Version du protocole pour compatibilité future
    pub protocol_version: u8,
    
    /// Type de paquet (Audio, Heartbeat, Handshake)
    pub packet_type: PacketType,
    
    /// ID unique du sender (pour support multi-peer futur)
    pub sender_id: u32,
    
    /// ID de session pour détecter les reconnexions
    pub session_id: u32,
    
    /// Frame audio compressée transportée
    pub compressed_frame: CompressedFrame,
    
    /// Timestamp d'envoi pour calcul RTT et latence
    /// Skip la sérialisation car Instant n'est pas portable entre machines
    /// Utilise le moment actuel lors de la désérialisation
    #[serde(skip, default = "Instant::now")]
    pub send_timestamp: Instant,
    
    /// Checksum simple pour détecter la corruption
    pub checksum: u32,
}

impl NetworkPacket {
    /// Version actuelle du protocole
    pub const CURRENT_PROTOCOL_VERSION: u8 = 1;
    
    /// Taille maximum autorisée pour un paquet (MTU safe)
    pub const MAX_PACKET_SIZE: usize = 1400;
    
    /// Crée un nouveau paquet audio
    /// 
    /// # Arguments
    /// * `compressed_frame` - La frame audio compressée à transporter
    /// * `sender_id` - ID unique de l'expéditeur
    /// * `session_id` - ID de la session courante
    /// 
    /// # Example
    /// ```rust
    /// use network::{NetworkPacket, PacketType};
    /// use audio::CompressedFrame;
    /// use std::time::Instant;
    /// 
    /// let frame = CompressedFrame::new(vec![1, 2, 3], 960, Instant::now(), 42);
    /// let packet = NetworkPacket::new_audio(frame, 123, 456);
    /// ```
    pub fn new_audio(compressed_frame: CompressedFrame, sender_id: u32, session_id: u32) -> Self {
        let mut packet = Self {
            protocol_version: Self::CURRENT_PROTOCOL_VERSION,
            packet_type: PacketType::Audio,
            sender_id,
            session_id,
            compressed_frame,
            send_timestamp: Instant::now(),
            checksum: 0,
        };
        
        packet.checksum = packet.calculate_checksum();
        packet
    }
    
    /// Crée un paquet heartbeat (keep-alive)
    pub fn new_heartbeat(sender_id: u32, session_id: u32) -> Self {
        // Frame vide pour heartbeat
        let empty_frame = CompressedFrame::new(vec![], 0, Instant::now(), 0);
        
        let mut packet = Self {
            protocol_version: Self::CURRENT_PROTOCOL_VERSION,
            packet_type: PacketType::Heartbeat,
            sender_id,
            session_id,
            compressed_frame: empty_frame,
            send_timestamp: Instant::now(),
            checksum: 0,
        };
        
        packet.checksum = packet.calculate_checksum();
        packet
    }
    
    /// Calcule un checksum simple pour détecter les erreurs
    /// 
    /// Utilise un XOR des bytes du paquet (simple mais efficace pour UDP)
    pub fn calculate_checksum(&self) -> u32 {
        let mut checksum = 0u32;
        checksum ^= self.protocol_version as u32;
        checksum ^= self.packet_type as u32;
        checksum ^= self.sender_id;
        checksum ^= self.session_id;
        checksum ^= self.compressed_frame.sequence_number as u32;
        checksum ^= self.compressed_frame.original_sample_count as u32;
        
        // XOR des données audio
        for chunk in self.compressed_frame.data.chunks(4) {
            let mut bytes = [0u8; 4];
            for (i, &b) in chunk.iter().enumerate() {
                bytes[i] = b;
            }
            checksum ^= u32::from_le_bytes(bytes);
        }
        
        checksum
    }
    
    /// Vérifie l'intégrité du paquet
    pub fn verify_checksum(&self) -> bool {
        self.checksum == self.calculate_checksum()
    }
    
    /// Calcule la taille sérialisée du paquet
    pub fn estimated_size(&self) -> usize {
        // Estimation basée sur la structure (pour éviter de sérialiser)
        32 + self.compressed_frame.data.len() // header + payload
    }
    
    /// Vérifie si le paquet est trop volumineux
    pub fn is_too_large(&self) -> bool {
        self.estimated_size() > Self::MAX_PACKET_SIZE
    }
    
    /// Calcule l'âge du paquet (temps depuis l'envoi)
    pub fn age(&self) -> Duration {
        self.send_timestamp.elapsed()
    }
    
    /// Vérifie si le paquet est "trop vieux"
    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.age() > max_age
    }
}

/// Types de paquets réseau
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum PacketType {
    /// Paquet contenant de l'audio
    Audio = 1,
    /// Paquet heartbeat (keep-alive)
    Heartbeat = 2,
    /// Paquet de handshake initial
    Handshake = 3,
    /// Paquet de disconnection propre
    Disconnect = 4,
}

/// États de connexion P2P
/// 
/// Représente l'état de la connexion entre deux pairs.
/// Utilisé pour la logique de retry et l'UI.
/// 
/// Note: Les champs `Instant` ne sont pas sérialisés car ils ne sont pas 
/// nécessaires pour la persistance et `Instant` n'implémente pas `Default`.
#[derive(Clone, Debug, PartialEq)]
pub enum ConnectionState {
    /// Aucune connexion active
    Disconnected,
    
    /// Tentative de connexion en cours
    Connecting { 
        target_addr: SocketAddr,
        started_at: Instant,
        attempt_count: u32,
    },
    
    /// Connexion établie et active
    Connected { 
        peer_addr: SocketAddr,
        session_id: u32,
        connected_at: Instant,
        last_heartbeat: Instant,
    },
    
    /// Erreur de connexion
    Error { 
        last_error: String,
        failed_at: Instant,
        can_retry: bool,
    },
}

impl ConnectionState {
    /// Vérifie si on est connecté
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionState::Connected { .. })
    }
    
    /// Vérifie si on est en train de se connecter
    pub fn is_connecting(&self) -> bool {
        matches!(self, ConnectionState::Connecting { .. })
    }
    
    /// Récupère l'adresse du peer si connecté
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        match self {
            ConnectionState::Connected { peer_addr, .. } => Some(*peer_addr),
            ConnectionState::Connecting { target_addr, .. } => Some(*target_addr),
            _ => None,
        }
    }
    
    /// Récupère le session ID si connecté
    pub fn session_id(&self) -> Option<u32> {
        match self {
            ConnectionState::Connected { session_id, .. } => Some(*session_id),
            _ => None,
        }
    }
    
    /// Description textuelle de l'état pour l'UI
    pub fn description(&self) -> String {
        match self {
            ConnectionState::Disconnected => "Déconnecté".to_string(),
            ConnectionState::Connecting { target_addr, attempt_count, .. } => {
                format!("Connexion vers {} (tentative {})", target_addr, attempt_count)
            }
            ConnectionState::Connected { peer_addr, .. } => {
                format!("Connecté à {}", peer_addr)
            }
            ConnectionState::Error { last_error, can_retry, .. } => {
                if *can_retry {
                    format!("Erreur (retry possible): {}", last_error)
                } else {
                    format!("Erreur fatale: {}", last_error)
                }
            }
        }
    }
}

/// Configuration du système réseau
/// 
/// Centralise tous les paramètres configurables du système réseau.
/// Permet d'ajuster les performances selon l'environnement (LAN vs WAN).
#[derive(Clone, Debug)]
pub struct NetworkConfig {
    /// Port d'écoute local (défaut: 9001)
    pub local_port: u16,
    
    /// Taille du buffer UDP en bytes (défaut: 64KB)
    pub socket_buffer_size: usize,
    
    /// Taille du buffer de réception en paquets (défaut: 100)
    pub receive_buffer_size: usize,
    
    /// Timeout pour les tentatives de connexion (défaut: 5s)
    pub connection_timeout: Duration,
    
    /// Intervalle entre les heartbeats (défaut: 1s)
    pub heartbeat_interval: Duration,
    
    /// Durée max sans heartbeat avant disconnection (défaut: 5s)
    pub heartbeat_timeout: Duration,
    
    /// Age maximum d'un paquet avant rejet (défaut: 100ms)
    pub max_packet_age: Duration,
    
    /// Nombre maximum de tentatives de reconnexion (défaut: 5)
    pub max_retry_attempts: u32,
    
    /// Délai entre les tentatives de reconnexion (défaut: 2s)
    pub retry_delay: Duration,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            local_port: 9001,
            socket_buffer_size: 65536, // 64KB
            receive_buffer_size: 100,  // ~100 frames = ~2s d'audio
            connection_timeout: Duration::from_secs(5),
            heartbeat_interval: Duration::from_secs(1),
            heartbeat_timeout: Duration::from_secs(5),
            max_packet_age: Duration::from_millis(100),
            max_retry_attempts: 5,
            retry_delay: Duration::from_secs(2),
        }
    }
}

impl NetworkConfig {
    /// Configuration optimisée pour LAN (latence faible)
    pub fn lan_optimized() -> Self {
        Self {
            heartbeat_interval: Duration::from_millis(500),
            heartbeat_timeout: Duration::from_secs(2),
            max_packet_age: Duration::from_millis(50),
            connection_timeout: Duration::from_secs(2),
            ..Default::default()
        }
    }
    
    /// Configuration pour WAN (plus tolérante)
    pub fn wan_optimized() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(2),
            heartbeat_timeout: Duration::from_secs(10),
            max_packet_age: Duration::from_millis(200),
            connection_timeout: Duration::from_secs(10),
            ..Default::default()
        }
    }
    
    /// Configuration pour tests (paramètres accélérés)
    pub fn test_config() -> Self {
        Self {
            heartbeat_interval: Duration::from_millis(100),
            heartbeat_timeout: Duration::from_millis(500),
            max_packet_age: Duration::from_millis(50),
            connection_timeout: Duration::from_millis(1000),
            max_retry_attempts: 2,
            retry_delay: Duration::from_millis(100),
            ..Default::default()
        }
    }
}

/// Statistiques réseau pour monitoring
/// 
/// Collecte des métriques sur les performances réseau.
/// Intégrable avec les AudioStats pour un monitoring global.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkStats {
    /// Nombre de paquets envoyés
    pub packets_sent: u64,
    
    /// Nombre de paquets reçus
    pub packets_received: u64,
    
    /// Nombre de paquets perdus (détectés par gap de séquence)
    pub packets_lost: u64,
    
    /// Nombre de paquets corrompus (checksum invalide)
    pub packets_corrupted: u64,
    
    /// Nombre de paquets rejetés (trop vieux)
    pub packets_rejected: u64,
    
    /// RTT moyen en millisecondes
    pub avg_rtt_ms: f32,
    
    /// Jitter réseau moyen (variation RTT)
    pub avg_jitter_ms: f32,
    
    /// Bande passante utilisée (bytes/sec)
    pub bandwidth_bytes_per_sec: f32,
    
    /// Nombre de reconnexions
    pub reconnection_count: u32,
    
    /// Durée de la connexion courante
    pub connection_uptime_ms: u64,
    
    /// Dernière mise à jour des stats
    /// Skip la sérialisation car Instant ne peut pas être sérialisé de manière portable
    /// Utilise une valeur par défaut lors de la désérialisation
    #[serde(skip, default = "Instant::now")]
    pub last_updated: Instant,
}

// Implementation manuelle de Default nécessaire car Instant n'implémente pas Default
// et serde a besoin de Default pour les champs avec #[serde(skip)]
impl Default for NetworkStats {
    fn default() -> Self {
        Self {
            packets_sent: 0,
            packets_received: 0,
            packets_lost: 0,
            packets_corrupted: 0,
            packets_rejected: 0,
            avg_rtt_ms: 0.0,
            avg_jitter_ms: 0.0,
            bandwidth_bytes_per_sec: 0.0,
            reconnection_count: 0,
            connection_uptime_ms: 0,
            last_updated: Instant::now(),
        }
    }
}

impl NetworkStats {
    /// Crée de nouvelles statistiques
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Remet les statistiques à zéro
    pub fn reset(&mut self) {
        *self = Self::new();
    }
    
    /// Calcule le pourcentage de perte de paquets
    pub fn loss_percentage(&self) -> f32 {
        if self.packets_sent == 0 {
            return 0.0;
        }
        (self.packets_lost as f32 / self.packets_sent as f32) * 100.0
    }
    
    /// Calcule le pourcentage de corruption
    pub fn corruption_percentage(&self) -> f32 {
        if self.packets_received == 0 {
            return 0.0;
        }
        (self.packets_corrupted as f32 / self.packets_received as f32) * 100.0
    }
    
    /// Évalue la qualité de la connexion réseau
    pub fn connection_quality(&self) -> ConnectionQuality {
        let loss_rate = self.loss_percentage();
        let corruption_rate = self.corruption_percentage();
        let rtt = self.avg_rtt_ms;
        
        if loss_rate > 10.0 || corruption_rate > 5.0 || rtt > 200.0 {
            ConnectionQuality::Poor
        } else if loss_rate > 5.0 || corruption_rate > 2.0 || rtt > 100.0 {
            ConnectionQuality::Fair
        } else if loss_rate > 1.0 || corruption_rate > 0.5 || rtt > 50.0 {
            ConnectionQuality::Good
        } else {
            ConnectionQuality::Excellent
        }
    }
}

/// Qualité de la connexion réseau
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Fair,
    Poor,
}

impl ConnectionQuality {
    /// Description textuelle pour l'UI
    pub fn description(&self) -> &'static str {
        match self {
            ConnectionQuality::Excellent => "Excellente",
            ConnectionQuality::Good => "Bonne",
            ConnectionQuality::Fair => "Moyenne",
            ConnectionQuality::Poor => "Mauvaise",
        }
    }
    
    /// Couleur suggérée pour l'UI
    pub fn color(&self) -> &'static str {
        match self {
            ConnectionQuality::Excellent => "green",
            ConnectionQuality::Good => "lightgreen",
            ConnectionQuality::Fair => "orange",
            ConnectionQuality::Poor => "red",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    
    #[test]
    fn test_network_packet_creation() {
        let frame = CompressedFrame::new(vec![1, 2, 3, 4], 960, Instant::now(), 42);
        let packet = NetworkPacket::new_audio(frame.clone(), 123, 456);
        
        assert_eq!(packet.protocol_version, NetworkPacket::CURRENT_PROTOCOL_VERSION);
        assert_eq!(packet.packet_type, PacketType::Audio);
        assert_eq!(packet.sender_id, 123);
        assert_eq!(packet.session_id, 456);
        assert_eq!(packet.compressed_frame.data, frame.data);
    }
    
    #[test]
    fn test_checksum_verification() {
        let frame = CompressedFrame::new(vec![1, 2, 3, 4], 960, Instant::now(), 42);
        let packet = NetworkPacket::new_audio(frame, 123, 456);
        
        assert!(packet.verify_checksum());
        
        // Test avec données modifiées
        let mut corrupted = packet.clone();
        corrupted.compressed_frame.data[0] = 99;
        assert!(!corrupted.verify_checksum());
    }
    
    #[test]
    fn test_connection_state() {
        let addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();
        
        let connecting = ConnectionState::Connecting {
            target_addr: addr,
            started_at: Instant::now(),
            attempt_count: 1,
        };
        assert!(connecting.is_connecting());
        assert!(!connecting.is_connected());
        assert_eq!(connecting.peer_addr(), Some(addr));
        
        let connected = ConnectionState::Connected {
            peer_addr: addr,
            session_id: 42,
            connected_at: Instant::now(),
            last_heartbeat: Instant::now(),
        };
        assert!(connected.is_connected());
        assert!(!connected.is_connecting());
        assert_eq!(connected.session_id(), Some(42));
    }
    
    #[test]
    fn test_network_config_presets() {
        let lan = NetworkConfig::lan_optimized();
        let wan = NetworkConfig::wan_optimized();
        
        // LAN doit avoir des timeouts plus courts
        assert!(lan.heartbeat_interval < wan.heartbeat_interval);
        assert!(lan.max_packet_age < wan.max_packet_age);
        
        // Test config a des paramètres encore plus rapides
        let test = NetworkConfig::test_config();
        assert!(test.connection_timeout < lan.connection_timeout);
        assert_eq!(test.max_retry_attempts, 2);
    }
    
    #[test]
    fn test_network_stats() {
        let mut stats = NetworkStats::new();
        
        // Test pourcentages à zéro
        assert_eq!(stats.loss_percentage(), 0.0);
        assert_eq!(stats.corruption_percentage(), 0.0);
        
        // Test avec quelques valeurs
        stats.packets_sent = 100;
        stats.packets_lost = 5;
        stats.packets_received = 95;
        stats.packets_corrupted = 2;
        
        assert_eq!(stats.loss_percentage(), 5.0);
        assert!((stats.corruption_percentage() - 2.105).abs() < 0.01); // 2/95 ≈ 2.105%
    }
    
    #[test]
    fn test_connection_quality() {
        let mut stats = NetworkStats::new();
        
        // Connexion excellente
        stats.avg_rtt_ms = 10.0;
        assert_eq!(stats.connection_quality(), ConnectionQuality::Excellent);
        
        // Connexion pauvre
        stats.avg_rtt_ms = 300.0;
        assert_eq!(stats.connection_quality(), ConnectionQuality::Poor);
        
        // Test avec perte de paquets
        stats.avg_rtt_ms = 20.0;
        stats.packets_sent = 100;
        stats.packets_lost = 15;
        assert_eq!(stats.connection_quality(), ConnectionQuality::Poor);
    }
    
    #[test]
    fn test_packet_age() {
        let frame = CompressedFrame::new(vec![1, 2, 3], 960, Instant::now(), 1);
        let packet = NetworkPacket::new_audio(frame, 123, 456);
        
        // Paquet juste créé
        assert!(packet.age() < Duration::from_millis(10));
        
        // Test staleness
        assert!(!packet.is_stale(Duration::from_secs(1)));
        
        // Simuler un vieux paquet
        let old_packet = {
            let mut p = packet.clone();
            p.send_timestamp = Instant::now() - Duration::from_secs(2);
            p
        };
        assert!(old_packet.is_stale(Duration::from_secs(1)));
    }
}
