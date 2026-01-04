//! Traits abstraits pour le système networking
//! 
//! Ce module définit les interfaces (traits) que doivent implémenter
//! tous les composants réseau. Cela permet d'avoir du code modulaire
//! et testable avec différentes implémentations.

use async_trait::async_trait;
use std::net::SocketAddr;
use crate::{NetworkPacket, NetworkStats, ConnectionState, NetworkResult};
use audio::CompressedFrame;

/// Trait pour le transport réseau bas niveau
/// 
/// Ce trait abstrait permet d'utiliser différentes implémentations :
/// - UdpTransport : Implémentation UDP réelle avec tokio
/// - MockTransport : Implémentation factice pour les tests
/// - SimulatedTransport : Simulation avec latence et perte de paquets
/// 
/// `#[async_trait]` permet d'avoir des fonctions async dans les traits.
/// `Send + Sync` indique que l'objet peut être transféré entre threads.
#[async_trait]
pub trait NetworkTransport: Send + Sync {
    /// Démarre le transport et bind sur le port local
    /// 
    /// Cette fonction initialise le socket UDP et commence à écouter.
    /// Elle doit être appelée avant toute autre opération réseau.
    /// 
    /// # Arguments
    /// * `local_port` - Port d'écoute local
    /// 
    /// # Erreurs
    /// - `NetworkError::BindError` : Impossible de bind sur le port
    /// - `NetworkError::InitializationError` : Échec de l'initialisation
    /// 
    /// # Example
    /// ```rust
    /// use network::{NetworkTransport, UdpTransport, NetworkConfig};
    /// 
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = NetworkConfig::default();
    /// let mut transport = UdpTransport::new(config)?;
    /// 
    /// transport.bind(9001).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn bind(&mut self, local_port: u16) -> NetworkResult<()>;
    
    /// Envoie un paquet vers une adresse spécifique
    /// 
    /// # Arguments
    /// * `packet` - Le paquet à envoyer
    /// * `target_addr` - Adresse de destination
    /// 
    /// # Erreurs
    /// - `NetworkError::PacketTooLarge` : Paquet trop volumineux
    /// - `NetworkError::IoError` : Erreur de transmission
    /// - `NetworkError::PeerDisconnected` : Destinataire injoignable
    async fn send_packet(&mut self, packet: &NetworkPacket, target_addr: SocketAddr) -> NetworkResult<()>;
    
    /// Reçoit le prochain paquet disponible
    /// 
    /// Cette fonction bloque jusqu'à ce qu'un paquet soit reçu ou qu'un timeout survienne.
    /// Elle valide automatiquement le checksum du paquet.
    /// 
    /// # Returns
    /// Un tuple (paquet, adresse expéditeur)
    /// 
    /// # Erreurs
    /// - `NetworkError::Timeout` : Pas de paquet reçu dans le délai
    /// - `NetworkError::CorruptedPacket` : Paquet avec checksum invalide
    /// - `NetworkError::InvalidPacketFormat` : Format de paquet invalide
    async fn receive_packet(&mut self) -> NetworkResult<(NetworkPacket, SocketAddr)>;
    
    /// Arrête le transport et libère les ressources
    async fn shutdown(&mut self) -> NetworkResult<()>;
    
    /// Retourne les statistiques de transport
    fn stats(&self) -> NetworkStats;
    
    /// Retourne l'adresse locale d'écoute
    fn local_addr(&self) -> Option<SocketAddr>;
    
    /// Vérifie si le transport est actif
    fn is_active(&self) -> bool;
}

/// Trait pour la gestion de connexion P2P haut niveau
/// 
/// Ce trait gère la logique métier de connexion peer-to-peer,
/// incluant les handshakes, heartbeats, et la récupération d'erreurs.
#[async_trait]
pub trait NetworkManager: Send + Sync {
    /// Démarre l'écoute en mode serveur
    /// 
    /// Le manager attend qu'un peer se connecte sur le port spécifié.
    /// 
    /// # Arguments
    /// * `port` - Port d'écoute
    /// 
    /// # Example
    /// ```rust,no_run
    /// use network::{NetworkManager, UdpNetworkManager, NetworkConfig};
    /// 
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = NetworkConfig::default();
    /// let mut manager = UdpNetworkManager::new(config)?;
    /// 
    /// manager.start_listening(9001).await?;
    /// println!("En attente de connexions...");
    /// # Ok(())
    /// # }
    /// ```
    async fn start_listening(&mut self, port: u16) -> NetworkResult<()>;
    
    /// Se connecte à un peer distant
    /// 
    /// Initie une connexion P2P avec handshake complet.
    /// 
    /// # Arguments
    /// * `peer_addr` - Adresse du peer distant (IP:PORT)
    /// 
    /// # Erreurs
    /// - `NetworkError::ConnectionTimeout` : Peer n'a pas répondu
    /// - `NetworkError::InvalidAddress` : Adresse invalide
    async fn connect_to_peer(&mut self, peer_addr: SocketAddr) -> NetworkResult<()>;
    
    /// Envoie une frame audio au peer connecté
    /// 
    /// # Arguments
    /// * `frame` - Frame audio compressée à envoyer
    /// 
    /// # Erreurs
    /// - `NetworkError::InvalidState` : Pas de connexion active
    /// - `NetworkError::BufferOverflow` : Buffer d'envoi plein
    async fn send_audio(&mut self, frame: CompressedFrame) -> NetworkResult<()>;
    
    /// Reçoit une frame audio du peer distant
    /// 
    /// Cette fonction bloque jusqu'à ce qu'une frame soit disponible.
    /// Elle gère automatiquement les paquets de contrôle (heartbeat, etc.).
    /// 
    /// # Returns
    /// Frame audio prête à être décodée
    /// 
    /// # Erreurs
    /// - `NetworkError::InvalidState` : Pas de connexion active
    /// - `NetworkError::PeerDisconnected` : Peer déconnecté
    /// - `NetworkError::BufferUnderflow` : Pas de données disponibles
    async fn receive_audio(&mut self) -> NetworkResult<CompressedFrame>;
    
    /// Déconnecte proprement du peer
    /// 
    /// Envoie un paquet de déconnexion et libère les ressources.
    async fn disconnect(&mut self) -> NetworkResult<()>;
    
    /// Retourne l'état de connexion actuel
    fn connection_state(&self) -> ConnectionState;
    
    /// Retourne les statistiques réseau combinées
    fn network_stats(&self) -> NetworkStats;
    
    /// Force une reconnexion si possible
    /// 
    /// Utile après une erreur réseau ou une coupure temporaire.
    async fn reconnect(&mut self) -> NetworkResult<()>;
}

/// Trait pour le monitoring réseau
/// 
/// Permet de collecter et analyser des métriques réseau en temps réel.
pub trait NetworkMonitor: Send + Sync {
    /// Enregistre l'envoi d'un paquet
    fn record_packet_sent(&mut self, packet: &NetworkPacket, target_addr: SocketAddr);
    
    /// Enregistre la réception d'un paquet
    fn record_packet_received(&mut self, packet: &NetworkPacket, source_addr: SocketAddr);
    
    /// Enregistre une perte de paquet détectée
    fn record_packet_lost(&mut self, sequence_number: u64);
    
    /// Enregistre un paquet corrompu
    fn record_packet_corrupted(&mut self, source_addr: SocketAddr);
    
    /// Enregistre une mesure de RTT
    fn record_rtt(&mut self, rtt_ms: f32);
    
    /// Enregistre une reconnexion
    fn record_reconnection(&mut self);
    
    /// Récupère les statistiques actuelles
    fn get_stats(&self) -> NetworkStats;
    
    /// Remet les statistiques à zéro
    fn reset_stats(&mut self);
    
    /// Calcule des métriques dérivées (jitter, qualité, etc.)
    fn calculate_derived_metrics(&mut self);
}

/// Trait pour les buffers réseau anti-jitter
/// 
/// Gère le buffering intelligent pour compenser les variations
/// de latence réseau et maintenir un flux audio stable.
#[async_trait]
pub trait NetworkBuffer: Send + Sync {
    /// Ajoute un paquet au buffer
    /// 
    /// Les paquets sont automatiquement triés par numéro de séquence.
    /// 
    /// # Arguments
    /// * `packet` - Paquet à ajouter
    /// 
    /// # Returns
    /// True si le paquet a été accepté, false s'il était en double ou trop ancien
    fn push_packet(&mut self, packet: NetworkPacket) -> bool;
    
    /// Récupère le prochain paquet disponible
    /// 
    /// Retourne le paquet suivant dans l'ordre de séquence,
    /// ou None si aucun paquet n'est prêt.
    fn pop_packet(&mut self) -> Option<NetworkPacket>;
    
    /// Vérifie s'il y a des paquets prêts à être lus
    fn has_packets(&self) -> bool;
    
    /// Retourne le niveau de remplissage du buffer (0.0 à 1.0)
    fn fill_level(&self) -> f32;
    
    /// Vide complètement le buffer
    /// 
    /// Utile après une reconnexion ou pour récupérer d'un décrochage.
    fn clear(&mut self);
    
    /// Configure la taille du buffer (en nombre de paquets)
    fn set_buffer_size(&mut self, size: usize);
    
    /// Retourne des statistiques sur le buffer
    fn buffer_stats(&self) -> BufferStats;
}

/// Statistiques du buffer réseau
#[derive(Clone, Debug, Default)]
pub struct BufferStats {
    /// Nombre de paquets en attente
    pub packets_buffered: usize,
    
    /// Nombre de paquets rejetés (trop anciens)
    pub packets_dropped: u64,
    
    /// Nombre de paquets en double rejetés
    pub duplicates_dropped: u64,
    
    /// Niveau de remplissage actuel (0.0 à 1.0)
    pub fill_level: f32,
    
    /// Jitter détecté (variation des délais)
    pub jitter_ms: f32,
    
    /// Délai d'attente moyen des paquets dans le buffer
    pub avg_delay_ms: f32,
}

/// Trait pour les implémentations de test et simulation
/// 
/// Permet de créer des environnements de test contrôlés pour
/// valider le comportement du système réseau.
#[async_trait]
pub trait NetworkSimulator: NetworkTransport {
    /// Configure la latence simulée
    async fn set_latency(&mut self, latency_ms: u32);
    
    /// Configure le taux de perte de paquets (0.0 à 1.0)
    async fn set_packet_loss_rate(&mut self, loss_rate: f32);
    
    /// Configure le jitter réseau (variation de latence)
    async fn set_jitter(&mut self, jitter_ms: u32);
    
    /// Simule une coupure réseau temporaire
    async fn simulate_network_outage(&mut self, duration_ms: u32);
    
    /// Active/désactive la duplication de paquets
    async fn set_packet_duplication(&mut self, enabled: bool, rate: f32);
    
    /// Simule une corruption de données aléatoire
    async fn set_corruption_rate(&mut self, corruption_rate: f32);
    
    /// Retourne les paramètres de simulation actuels
    fn simulation_params(&self) -> SimulationParams;
}

/// Paramètres de simulation réseau
#[derive(Clone, Debug, Default)]
pub struct SimulationParams {
    pub latency_ms: u32,
    pub loss_rate: f32,
    pub jitter_ms: u32,
    pub duplication_rate: f32,
    pub corruption_rate: f32,
    pub is_network_down: bool,
}

/// Trait pour les modes de test spéciaux
/// 
/// Fournit des méthodes utilitaires pour les tests automatisés.
#[async_trait]
pub trait NetworkTestMode: Send + Sync {
    /// Mode loopback : les paquets envoyés sont renvoyés à l'expéditeur
    async fn enable_loopback_mode(&mut self) -> NetworkResult<()>;
    
    /// Mode echo server : renvoie tout ce qui est reçu
    async fn enable_echo_mode(&mut self) -> NetworkResult<()>;
    
    /// Génère du trafic de test automatiquement
    async fn start_traffic_generator(&mut self, packets_per_second: u32) -> NetworkResult<()>;
    
    /// Arrête le générateur de trafic
    async fn stop_traffic_generator(&mut self) -> NetworkResult<()>;
    
    /// Lance un test de performance automatique
    async fn run_performance_test(&mut self, duration_seconds: u32) -> NetworkResult<PerformanceReport>;
}

/// Rapport de performance réseau
#[derive(Clone, Debug)]
pub struct PerformanceReport {
    pub test_duration_ms: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub avg_rtt_ms: f32,
    pub max_rtt_ms: f32,
    pub min_rtt_ms: f32,
    pub jitter_ms: f32,
    pub loss_percentage: f32,
    pub throughput_mbps: f32,
    pub recommendations: Vec<String>,
}

impl PerformanceReport {
    /// Évalue les résultats et génère des recommandations
    pub fn generate_recommendations(&mut self) {
        self.recommendations.clear();
        
        if self.loss_percentage > 5.0 {
            self.recommendations.push("Perte de paquets élevée - vérifier la qualité réseau".to_string());
        }
        
        if self.avg_rtt_ms > 100.0 {
            self.recommendations.push("Latence élevée - utiliser la configuration WAN".to_string());
        }
        
        if self.jitter_ms > 20.0 {
            self.recommendations.push("Jitter important - augmenter la taille du buffer".to_string());
        }
        
        if self.recommendations.is_empty() {
            self.recommendations.push("Performances réseau excellentes".to_string());
        }
    }
    
    /// Résumé textuel des résultats
    pub fn summary(&self) -> String {
        format!(
            "Test {} - RTT: {:.1}ms, Perte: {:.1}%, Débit: {:.1} Mbps",
            if self.test_duration_ms > 0 { "réussi" } else { "échoué" },
            self.avg_rtt_ms,
            self.loss_percentage,
            self.throughput_mbps
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Tests que les traits sont bien formés et utilisables
    
    #[test]
    fn test_buffer_stats() {
        let stats = BufferStats::default();
        assert_eq!(stats.packets_buffered, 0);
        assert_eq!(stats.fill_level, 0.0);
    }
    
    #[test]
    fn test_simulation_params() {
        let params = SimulationParams::default();
        assert_eq!(params.latency_ms, 0);
        assert_eq!(params.loss_rate, 0.0);
        assert!(!params.is_network_down);
    }
    
    #[test]
    fn test_performance_report() {
        let mut report = PerformanceReport {
            test_duration_ms: 10000,
            packets_sent: 1000,
            packets_received: 950,
            avg_rtt_ms: 25.0,
            max_rtt_ms: 50.0,
            min_rtt_ms: 10.0,
            jitter_ms: 5.0,
            loss_percentage: 5.0,
            throughput_mbps: 1.2,
            recommendations: vec![],
        };
        
        report.generate_recommendations();
        assert!(!report.recommendations.is_empty());
        
        let summary = report.summary();
        assert!(summary.contains("25.0ms"));
        assert!(summary.contains("5.0%"));
    }
}
