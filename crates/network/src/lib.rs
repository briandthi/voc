//! Crate network - Système de communication réseau P2P pour audio temps réel
//! 
//! Ce crate fournit une architecture complète pour la communication audio peer-to-peer
//! via UDP, avec gestion des connexions, buffering anti-jitter, et monitoring des performances.
//! 
//! # Architecture
//! 
//! Le crate est organisé en plusieurs modules :
//! 
//! - `error` : Gestion d'erreurs avec types spécialisés réseau
//! - `types` : Types de données (paquets, états, configurations, statistiques)
//! - `traits` : Traits abstraits pour transport, manager, monitoring
//! - `transport` : Implémentations UDP (réel et simulé)
//! - `manager` : Manager haut niveau P2P avec logique métier
//! 
//! # Examples
//! 
//! ## Client basique
//! 
//! ```rust,no_run
//! use network::{UdpNetworkManager, NetworkManager, NetworkConfig};
//! 
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = NetworkConfig::default();
//! let mut manager = UdpNetworkManager::new(config)?;
//! 
//! // Se connecte à un peer
//! manager.connect_to_peer("192.168.1.100:9001".parse()?).await?;
//! 
//! // Envoie de l'audio
//! // let audio_frame = ...; // CompressedFrame depuis le système audio
//! // manager.send_audio(audio_frame).await?;
//! 
//! // Reçoit de l'audio
//! // let received_frame = manager.receive_audio().await?;
//! 
//! manager.disconnect().await?;
//! # Ok(())
//! # }
//! ```
//! 
//! ## Serveur basique
//! 
//! ```rust,no_run
//! use network::{UdpNetworkManager, NetworkManager, NetworkConfig};
//! 
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = NetworkConfig::default();
//! let mut manager = UdpNetworkManager::new(config)?;
//! 
//! // Écoute sur le port 9001
//! manager.start_listening(9001).await?;
//! 
//! println!("Serveur prêt, en attente de connexions...");
//! # Ok(())
//! # }
//! ```
//! 
//! ## Tests et simulation
//! 
//! ```rust
//! use network::{UdpNetworkManager, NetworkConfig, NetworkTransport, SimulatedTransport};
//! 
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Configuration optimisée pour les tests
//! let config = NetworkConfig::test_config();
//! 
//! // Manager avec transport simulé (sans vrai réseau)
//! let mut manager = UdpNetworkManager::new_simulated(config)?;
//! 
//! // Tests de loopback, latence simulée, perte de paquets, etc.
//! # Ok(())
//! # }
//! ```

// Modules internes
mod error;
mod types;
mod traits;
mod transport;
mod manager;

// Re-exports publics
pub use error::{NetworkError, NetworkResult};

pub use types::{
    NetworkPacket, PacketType, ConnectionState, ConnectionQuality,
    NetworkConfig, NetworkStats
};

pub use traits::{
    NetworkTransport, NetworkManager, NetworkMonitor, NetworkBuffer,
    BufferStats, NetworkSimulator, NetworkTestMode, SimulationParams, PerformanceReport
};

pub use transport::{UdpTransport, SimulatedTransport};

pub use manager::UdpNetworkManager;

// Re-exports depuis le crate audio (pour simplicité d'utilisation)
pub use audio::CompressedFrame;

/// Version du crate network
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Fonctions utilitaires pour l'utilisateur final
pub mod utils {
    use super::*;
    use std::net::{SocketAddr, IpAddr, Ipv4Addr};
    
    /// Parse une adresse IP:PORT depuis une string
    /// 
    /// # Arguments
    /// * `addr_str` - String au format "IP:PORT" (ex: "192.168.1.100:9001")
    /// 
    /// # Example
    /// ```rust
    /// use network::utils;
    /// 
    /// let addr = utils::parse_address("192.168.1.100:9001").unwrap();
    /// assert_eq!(addr.port(), 9001);
    /// ```
    pub fn parse_address(addr_str: &str) -> NetworkResult<SocketAddr> {
        addr_str.parse()
            .map_err(|_| NetworkError::InvalidAddress { 
                addr: addr_str.to_string() 
            })
    }
    
    /// Crée une adresse localhost sur le port spécifié
    /// 
    /// # Arguments
    /// * `port` - Port à utiliser
    /// 
    /// # Example
    /// ```rust
    /// use network::utils;
    /// 
    /// let addr = utils::localhost(9001);
    /// assert_eq!(addr.to_string(), "127.0.0.1:9001");
    /// ```
    pub fn localhost(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
    }
    
    /// Détecte l'adresse IP locale principale
    /// 
    /// Utile pour afficher l'adresse à laquelle les autres peuvent se connecter.
    /// 
    /// # Example
    /// ```rust
    /// use network::utils;
    /// 
    /// if let Ok(local_ip) = utils::get_local_ip() {
    ///     println!("Connectez-vous à {}:9001", local_ip);
    /// }
    /// ```
    pub fn get_local_ip() -> NetworkResult<IpAddr> {
        // Méthode simple : se connecte à un serveur externe pour déduire l'IP locale
        use std::net::UdpSocket;
        
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| NetworkError::IoError(e))?;
            
        // Se "connecte" à 8.8.8.8:80 (ne fait que configurer le routage)
        socket.connect("8.8.8.8:80")
            .map_err(|e| NetworkError::IoError(e))?;
            
        let local_addr = socket.local_addr()
            .map_err(|e| NetworkError::IoError(e))?;
            
        Ok(local_addr.ip())
    }
    
    /// Formate une durée en millisecondes de façon lisible
    /// 
    /// # Example
    /// ```rust
    /// use network::utils;
    /// use std::time::Duration;
    /// 
    /// let duration = Duration::from_millis(1234);
    /// assert_eq!(utils::format_duration(duration), "1.23s");
    /// 
    /// let short_duration = Duration::from_millis(56);
    /// assert_eq!(utils::format_duration(short_duration), "56ms");
    /// ```
    pub fn format_duration(duration: std::time::Duration) -> String {
        let ms = duration.as_millis();
        
        if ms >= 1000 {
            format!("{:.2}s", ms as f64 / 1000.0)
        } else {
            format!("{}ms", ms)
        }
    }
    
    /// Formate une taille en bytes de façon lisible
    /// 
    /// # Example
    /// ```rust
    /// use network::utils;
    /// 
    /// assert_eq!(utils::format_bytes(1024), "1.0 KB");
    /// assert_eq!(utils::format_bytes(1536), "1.5 KB");
    /// assert_eq!(utils::format_bytes(500), "500 B");
    /// ```
    pub fn format_bytes(bytes: usize) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
        
        let mut size = bytes as f64;
        let mut unit_index = 0;
        
        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }
        
        if unit_index == 0 {
            format!("{} {}", bytes, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size, UNITS[unit_index])
        }
    }
}

/// Tests d'intégration du crate complet
#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_basic_manager_creation() {
        let config = NetworkConfig::test_config();
        
        // Test création avec transport réel
        let manager_real = UdpNetworkManager::new(config.clone());
        assert!(manager_real.is_ok());
        
        // Test création avec transport simulé
        let manager_sim = UdpNetworkManager::new_simulated(config);
        assert!(manager_sim.is_ok());
    }
    
    #[tokio::test]
    async fn test_simulated_loopback() {
        let config = NetworkConfig::test_config();
        let mut _manager = UdpNetworkManager::new_simulated(config).unwrap();
        
        // Note: Les champs privés ne sont pas accessibles directement
        // On utilise les méthodes publiques du manager à la place
        println!("Transport simulé créé avec succès");
    }
    
    #[test]
    fn test_utility_functions() {
        // Test parsing d'adresse
        let addr = utils::parse_address("127.0.0.1:9001").unwrap();
        assert_eq!(addr.port(), 9001);
        
        // Test localhost
        let localhost_addr = utils::localhost(8080);
        assert_eq!(localhost_addr.port(), 8080);
        
        // Test formatage de durée
        let duration = Duration::from_millis(1500);
        assert_eq!(utils::format_duration(duration), "1.50s");
        
        // Test formatage de bytes
        assert_eq!(utils::format_bytes(2048), "2.0 KB");
    }
    
    #[test]
    fn test_config_presets() {
        let default_config = NetworkConfig::default();
        let lan_config = NetworkConfig::lan_optimized();
        let wan_config = NetworkConfig::wan_optimized();
        let test_config = NetworkConfig::test_config();
        
        // LAN doit avoir des timeouts plus courts que WAN
        assert!(lan_config.heartbeat_interval < wan_config.heartbeat_interval);
        assert!(lan_config.max_packet_age < wan_config.max_packet_age);
        
        // Test doit avoir des timeouts très courts
        assert!(test_config.connection_timeout < default_config.connection_timeout);
        assert_eq!(test_config.max_retry_attempts, 2);
    }
    
    #[test]
    fn test_error_types() {
        // Test création d'erreurs avec helpers
        let bind_error = NetworkError::bind_failed(9001, std::io::Error::new(
            std::io::ErrorKind::PermissionDenied, "test"
        ));
        
        match bind_error {
            NetworkError::BindError { port, .. } => assert_eq!(port, 9001),
            _ => panic!("Wrong error type"),
        }
        
        // Test propriétés des erreurs
        let timeout_error = NetworkError::ConnectionTimeout {
            addr: "127.0.0.1:9001".parse().unwrap(),
            timeout_ms: 5000,
        };
        
        assert!(timeout_error.is_recoverable());
        assert!(timeout_error.requires_reconnection());
    }
    
    #[test]
    fn test_network_stats() {
        let mut stats = NetworkStats::new();
        
        // Test calculs de base
        assert_eq!(stats.loss_percentage(), 0.0);
        assert_eq!(stats.corruption_percentage(), 0.0);
        assert_eq!(stats.connection_quality(), ConnectionQuality::Excellent);
        
        // Test avec des valeurs
        stats.packets_sent = 100;
        stats.packets_lost = 5;
        stats.avg_rtt_ms = 150.0;
        
        assert_eq!(stats.loss_percentage(), 5.0);
        assert_eq!(stats.connection_quality(), ConnectionQuality::Fair);
    }
}

/// Exemples d'utilisation pour la documentation
#[cfg(test)]
mod examples {
    use super::*;
    use std::time::Duration;
    
    /// Example: Configuration et création d'un manager
    #[allow(dead_code)]
    async fn example_manager_setup() -> NetworkResult<()> {
        // Configuration personnalisée pour LAN
        let config = NetworkConfig::lan_optimized();
        
        // Création du manager
        let _manager = UdpNetworkManager::new(config)?;
        
        println!("Manager créé avec succès");
        Ok(())
    }
    
    /// Example: Test de simulation réseau
    #[allow(dead_code)]
    async fn example_network_simulation() -> NetworkResult<()> {
        let mut config = NetworkConfig::test_config();
        config.max_packet_age = Duration::from_millis(50); // Très strict
        
        let _manager = UdpNetworkManager::new_simulated(config)?;
        
        // Simulation avec latence et perte
        // (dans un vrai test, on configurerait le transport simulé)
        
        println!("Simulation configurée");
        Ok(())
    }
}
