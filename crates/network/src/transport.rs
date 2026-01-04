//! Transport UDP pour communication P2P
//! 
//! Ce module implémente le transport réseau bas niveau utilisant UDP avec tokio.
//! Il fournit une implémentation concrète du trait NetworkTransport avec toutes
//! les fonctionnalités nécessaires pour une communication audio temps réel.

use async_trait::async_trait;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
use std::time::Instant;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    NetworkTransport, NetworkPacket, NetworkStats, NetworkConfig, NetworkResult, NetworkError
};

/// Implémentation du transport UDP avec tokio
/// 
/// Cette structure gère la communication UDP bidirectionnelle pour le transfert
/// d'audio en peer-to-peer. Elle inclut la sérialisation/désérialisation des paquets,
/// la validation des checksums, et le monitoring des performances.
/// 
/// # Architecture
/// - Socket UDP non-bloquant avec tokio
/// - Buffer configurable pour optimiser les performances
/// - Validation automatique des paquets (checksum, taille)
/// - Statistiques temps réel pour monitoring
/// 
/// # Example
/// ```rust,no_run
/// use network::{UdpTransport, NetworkConfig, NetworkTransport};
/// 
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = NetworkConfig::default();
/// let mut transport = UdpTransport::new(config)?;
/// 
/// transport.bind(9001).await?;
/// println!("Transport UDP actif sur port 9001");
/// # Ok(())
/// # }
/// ```
pub struct UdpTransport {
    /// Configuration réseau
    config: NetworkConfig,
    
    /// Socket UDP tokio (partagé entre threads)
    socket: Option<Arc<UdpSocket>>,
    
    /// Statistiques réseau
    stats: Arc<Mutex<NetworkStats>>,
    
    /// Buffer temporaire pour la sérialisation
    send_buffer: Vec<u8>,
    
    /// Buffer temporaire pour la réception
    receive_buffer: Vec<u8>,
    
    /// Adresse locale d'écoute
    local_addr: Option<SocketAddr>,
    
    /// Indique si le transport est actif
    is_active: bool,
}

impl UdpTransport {
    /// Crée une nouvelle instance de transport UDP
    /// 
    /// # Arguments
    /// * `config` - Configuration réseau
    /// 
    /// # Returns
    /// Transport UDP prêt à être utilisé (mais pas encore bind)
    /// 
    /// # Example
    /// ```rust
    /// use network::{UdpTransport, NetworkConfig};
    /// 
    /// let config = NetworkConfig::default();
    /// let transport = UdpTransport::new(config).unwrap();
    /// ```
    pub fn new(config: NetworkConfig) -> NetworkResult<Self> {
        Ok(Self {
            config,
            socket: None,
            stats: Arc::new(Mutex::new(NetworkStats::new())),
            send_buffer: Vec::with_capacity(2048), // Pré-alloue pour éviter des réallocations
            receive_buffer: vec![0u8; 2048],
            local_addr: None,
            is_active: false,
        })
    }
    
    /// Sérialise un paquet en bytes pour transmission
    /// 
    /// Utilise bincode pour une sérialisation efficace et compacte.
    /// Met à jour le send_timestamp avant sérialisation et recalcule le checksum.
    fn serialize_packet(&mut self, packet: &mut NetworkPacket) -> NetworkResult<&[u8]> {
        // Met à jour le timestamp d'envoi
        packet.send_timestamp = Instant::now();
        
        // Recalcule le checksum du paquet réel (après modification du timestamp)
        // CORRECTION: Il faut calculer le checksum du paquet actuel, pas d'un paquet temporaire
        packet.checksum = packet.calculate_checksum();
        
        // Sérialise dans le buffer pré-alloué
        self.send_buffer.clear();
        
        match bincode::serialize_into(&mut self.send_buffer, packet) {
            Ok(()) => {
                // Vérification de la taille
                if self.send_buffer.len() > NetworkPacket::MAX_PACKET_SIZE {
                    return Err(NetworkError::packet_too_large(
                        self.send_buffer.len(),
                        NetworkPacket::MAX_PACKET_SIZE,
                    ));
                }
                Ok(&self.send_buffer)
            }
            Err(e) => Err(NetworkError::SerializationError(e)),
        }
    }
    
    /// Désérialise des bytes en paquet
    /// 
    /// Valide automatiquement le checksum et la version du protocole.
    fn deserialize_packet(&self, data: &[u8], source_addr: SocketAddr) -> NetworkResult<NetworkPacket> {
        // Désérialisation
        let packet: NetworkPacket = bincode::deserialize(data)
            .map_err(|_| NetworkError::InvalidPacketFormat { addr: source_addr })?;
        
        // Validation de la version du protocole
        if packet.protocol_version != NetworkPacket::CURRENT_PROTOCOL_VERSION {
            return Err(NetworkError::InvalidPacketFormat { addr: source_addr });
        }
        
        // Validation du checksum
        if !packet.verify_checksum() {
            return Err(NetworkError::corrupted_packet(source_addr));
        }
        
        // Vérification de l'âge du paquet
        if packet.is_stale(self.config.max_packet_age) {
            return Err(NetworkError::PacketTooOld {
                sequence: packet.compressed_frame.sequence_number,
                age_ms: packet.age().as_millis() as u64,
            });
        }
        
        Ok(packet)
    }
    
    /// Met à jour les statistiques après envoi d'un paquet
    async fn update_send_stats(&self, packet: &NetworkPacket, _target_addr: SocketAddr) {
        let mut stats = self.stats.lock().await;
        stats.packets_sent += 1;
        stats.last_updated = Instant::now();
        
        // Mise à jour de la bande passante
        let packet_size = packet.estimated_size() as f32;
        let elapsed = stats.last_updated.duration_since(Instant::now() - Duration::from_secs(1));
        if elapsed.as_secs_f32() > 0.0 {
            stats.bandwidth_bytes_per_sec = packet_size / elapsed.as_secs_f32();
        }
    }
    
    /// Met à jour les statistiques après réception d'un paquet
    async fn update_receive_stats(&self, packet: &NetworkPacket, _source_addr: SocketAddr) {
        let mut stats = self.stats.lock().await;
        stats.packets_received += 1;
        stats.last_updated = Instant::now();
        
        // Calcul du RTT si c'est un paquet de type heartbeat
        if matches!(packet.packet_type, crate::PacketType::Heartbeat) {
            let rtt_ms = packet.age().as_millis() as f32;
            
            // Mise à jour du RTT moyen (moyenne mobile)
            if stats.avg_rtt_ms == 0.0 {
                stats.avg_rtt_ms = rtt_ms;
            } else {
                stats.avg_rtt_ms = stats.avg_rtt_ms * 0.8 + rtt_ms * 0.2;
            }
            
            // Calcul du jitter (variation du RTT)
            let jitter = (rtt_ms - stats.avg_rtt_ms).abs();
            if stats.avg_jitter_ms == 0.0 {
                stats.avg_jitter_ms = jitter;
            } else {
                stats.avg_jitter_ms = stats.avg_jitter_ms * 0.8 + jitter * 0.2;
            }
        }
    }
}

#[async_trait]
impl NetworkTransport for UdpTransport {
    /// Bind le socket UDP sur le port local
    /// 
    /// Cette fonction crée et configure le socket UDP pour l'écoute.
    /// Elle configure aussi les buffers système pour optimiser les performances.
    async fn bind(&mut self, local_port: u16) -> NetworkResult<()> {
        if self.socket.is_some() {
            return Err(NetworkError::InvalidState {
                operation: "bind".to_string(),
                current_state: "already bound".to_string(),
            });
        }
        
        // Création du socket
        let addr = SocketAddr::from(([0, 0, 0, 0], local_port));
        let socket = UdpSocket::bind(addr).await
            .map_err(|e| NetworkError::bind_failed(local_port, e))?;
        
        // Configuration des buffers système (non disponible avec tokio::net::UdpSocket)
        // Les buffers seront configurés par le système d'exploitation
        
        // Récupération de l'adresse locale réelle
        self.local_addr = socket.local_addr().ok();
        
        // Stockage du socket
        self.socket = Some(Arc::new(socket));
        self.is_active = true;
        
        println!("Transport UDP bind sur {}", self.local_addr.unwrap());
        Ok(())
    }
    
    /// Envoie un paquet vers une adresse cible
    /// 
    /// La fonction sérialise le paquet, l'envoie via UDP, et met à jour les statistiques.
    async fn send_packet(&mut self, packet: &NetworkPacket, target_addr: SocketAddr) -> NetworkResult<()> {
        // Vérification de l'état avant toute opération
        let socket = self.socket.as_ref()
            .ok_or_else(|| NetworkError::InvalidState {
                operation: "send_packet".to_string(),
                current_state: "not bound".to_string(),
            })?
            .clone(); // Clone l'Arc pour éviter les conflits d'emprunts
        
        // Copie du timeout pour éviter l'emprunt de self.config
        let connection_timeout = self.config.connection_timeout;
        
        // Copie le paquet pour pouvoir le modifier (timestamp)
        let mut packet_to_send = packet.clone();
        
        // Sérialisation (maintenant safe car on a cloné les références nécessaires)
        let data = self.serialize_packet(&mut packet_to_send)?;
        
        // Envoi avec timeout
        let send_result = timeout(
            connection_timeout,
            socket.send_to(data, target_addr)
        ).await;
        
        match send_result {
            Ok(Ok(bytes_sent)) => {
                // Vérification que tous les bytes ont été envoyés
                if bytes_sent != data.len() {
                    return Err(NetworkError::IoError(
                        std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            "Envoi incomplet"
                        )
                    ));
                }
                
                // Mise à jour des statistiques
                self.update_send_stats(&packet_to_send, target_addr).await;
                
                Ok(())
            }
            Ok(Err(e)) => Err(NetworkError::IoError(e)),
            Err(_) => Err(NetworkError::ConnectionTimeout {
                addr: target_addr,
                timeout_ms: self.config.connection_timeout.as_millis() as u32,
            }),
        }
    }
    
    /// Reçoit le prochain paquet disponible
    /// 
    /// Cette fonction bloque jusqu'à réception d'un paquet valide ou timeout.
    async fn receive_packet(&mut self) -> NetworkResult<(NetworkPacket, SocketAddr)> {
        let socket = self.socket.as_ref()
            .ok_or_else(|| NetworkError::InvalidState {
                operation: "receive_packet".to_string(),
                current_state: "not bound".to_string(),
            })?;
        
        // Réception avec timeout
        let receive_result = timeout(
            self.config.connection_timeout,
            socket.recv_from(&mut self.receive_buffer)
        ).await;
        
        match receive_result {
            Ok(Ok((bytes_received, source_addr))) => {
                // Désérialisation et validation
                let packet = self.deserialize_packet(
                    &self.receive_buffer[..bytes_received],
                    source_addr
                )?;
                
                // Mise à jour des statistiques
                self.update_receive_stats(&packet, source_addr).await;
                
                Ok((packet, source_addr))
            }
            Ok(Err(e)) => Err(NetworkError::IoError(e)),
            Err(_) => Err(NetworkError::Timeout),
        }
    }
    
    /// Arrête le transport et libère les ressources
    async fn shutdown(&mut self) -> NetworkResult<()> {
        self.socket = None;
        self.local_addr = None;
        self.is_active = false;
        
        // Reset des statistiques
        let mut stats = self.stats.lock().await;
        stats.reset();
        
        println!("Transport UDP arrêté");
        Ok(())
    }
    
    /// Retourne les statistiques courantes
    fn stats(&self) -> NetworkStats {
        // Version synchrone - on utilise try_lock pour éviter de bloquer
        match self.stats.try_lock() {
            Ok(stats) => stats.clone(),
            Err(_) => NetworkStats::default(), // Si le lock échoue, retourne des stats vides
        }
    }
    
    /// Retourne l'adresse locale d'écoute
    fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }
    
    /// Vérifie si le transport est actif
    fn is_active(&self) -> bool {
        self.is_active && self.socket.is_some()
    }
}

/// Implémentation de transport simulé pour les tests
/// 
/// Cette implémentation permet de tester le comportement réseau
/// en simulant différentes conditions (latence, perte, etc.).
pub struct SimulatedTransport {
    /// Configuration de base
    config: NetworkConfig,
    
    /// Paramètres de simulation
    latency_ms: u32,
    loss_rate: f32,
    jitter_ms: u32,
    corruption_rate: f32,
    
    /// Buffer interne pour simuler la réception
    receive_queue: std::collections::VecDeque<(NetworkPacket, SocketAddr)>,
    
    /// Statistiques
    stats: NetworkStats,
    
    /// État du transport
    is_active: bool,
    local_addr: Option<SocketAddr>,
}

impl SimulatedTransport {
    /// Crée un nouveau transport simulé
    pub fn new(config: NetworkConfig) -> NetworkResult<Self> {
        Ok(Self {
            config,
            latency_ms: 0,
            loss_rate: 0.0,
            jitter_ms: 0,
            corruption_rate: 0.0,
            receive_queue: std::collections::VecDeque::new(),
            stats: NetworkStats::new(),
            is_active: false,
            local_addr: None,
        })
    }
    
    /// Configure les paramètres de simulation
    pub fn set_simulation_params(&mut self, latency_ms: u32, loss_rate: f32, jitter_ms: u32) {
        self.latency_ms = latency_ms;
        self.loss_rate = loss_rate;
        self.jitter_ms = jitter_ms;
    }
    
    /// Simule l'envoi d'un paquet vers soi-même (loopback)
    fn simulate_loopback(&mut self, packet: NetworkPacket, target_addr: SocketAddr) {
        // Simulation de perte de paquets
        if fastrand::f32() < self.loss_rate {
            self.stats.packets_lost += 1;
            return;
        }
        
        // Simulation de latence
        let _actual_latency = if self.jitter_ms > 0 {
            self.latency_ms + fastrand::u32(0..self.jitter_ms)
        } else {
            self.latency_ms
        };
        
        // Pour simplifier, on ajoute directement dans la queue
        // Dans un vrai simulateur, on utiliserait un timer
        self.receive_queue.push_back((packet, target_addr));
        self.stats.packets_sent += 1;
    }
}

#[async_trait]
impl NetworkTransport for SimulatedTransport {
    async fn bind(&mut self, local_port: u16) -> NetworkResult<()> {
        self.local_addr = Some(SocketAddr::from(([127, 0, 0, 1], local_port)));
        self.is_active = true;
        println!("Transport simulé bind sur port {}", local_port);
        Ok(())
    }
    
    async fn send_packet(&mut self, packet: &NetworkPacket, target_addr: SocketAddr) -> NetworkResult<()> {
        if !self.is_active {
            return Err(NetworkError::InvalidState {
                operation: "send_packet".to_string(),
                current_state: "not active".to_string(),
            });
        }
        
        // Simulation de corruption
        let mut packet_copy = packet.clone();
        if fastrand::f32() < self.corruption_rate {
            // Corrompt le checksum
            packet_copy.checksum = 0xDEADBEEF;
        }
        
        self.simulate_loopback(packet_copy, target_addr);
        Ok(())
    }
    
    async fn receive_packet(&mut self) -> NetworkResult<(NetworkPacket, SocketAddr)> {
        if !self.is_active {
            return Err(NetworkError::InvalidState {
                operation: "receive_packet".to_string(),
                current_state: "not active".to_string(),
            });
        }
        
        // Simulation d'attente
        if self.latency_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.latency_ms as u64)).await;
        }
        
        // Utilisation du timeout de configuration
        match timeout(self.config.connection_timeout, async {
            loop {
                if let Some((packet, addr)) = self.receive_queue.pop_front() {
                    self.stats.packets_received += 1;
                    return Ok((packet, addr));
                }
                // Simulation d'attente active
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await {
            Ok(result) => result,
            Err(_) => Err(NetworkError::Timeout),
        }
    }
    
    async fn shutdown(&mut self) -> NetworkResult<()> {
        self.is_active = false;
        self.receive_queue.clear();
        self.stats.reset();
        println!("Transport simulé arrêté");
        Ok(())
    }
    
    fn stats(&self) -> NetworkStats {
        self.stats.clone()
    }
    
    fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }
    
    fn is_active(&self) -> bool {
        self.is_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    
    #[test]
    fn test_udp_transport_creation() {
        let config = NetworkConfig::default();
        let transport = UdpTransport::new(config).unwrap();
        
        assert!(!transport.is_active());
        assert!(transport.local_addr().is_none());
    }
    
    #[test]
    fn test_simulated_transport_creation() {
        let config = NetworkConfig::default();
        let mut transport = SimulatedTransport::new(config).unwrap();
        
        assert!(!transport.is_active());
        
        // Test paramètres de simulation
        transport.set_simulation_params(50, 0.1, 10);
        assert_eq!(transport.latency_ms, 50);
        assert!((transport.loss_rate - 0.1).abs() < 0.001);
        assert_eq!(transport.jitter_ms, 10);
    }
    
    #[tokio::test]
    async fn test_simulated_transport_bind() {
        let config = NetworkConfig::default();
        let mut transport = SimulatedTransport::new(config).unwrap();
        
        transport.bind(9001).await.unwrap();
        
        assert!(transport.is_active());
        assert_eq!(transport.local_addr(), Some("127.0.0.1:9001".parse().unwrap()));
    }
    
    #[tokio::test]
    async fn test_packet_serialization() {
        use crate::{NetworkPacket};
        use audio::CompressedFrame;
        
        let config = NetworkConfig::default();
        let mut transport = UdpTransport::new(config).unwrap();
        
        let frame = CompressedFrame::new(vec![1, 2, 3, 4], 960, Instant::now(), 42);
        let mut packet = NetworkPacket::new_audio(frame, 123, 456);
        
        let serialized = transport.serialize_packet(&mut packet).unwrap();
        assert!(!serialized.is_empty());
        assert!(serialized.len() < NetworkPacket::MAX_PACKET_SIZE);
    }
    
    #[tokio::test]
    async fn test_packet_validation() {
        let config = NetworkConfig::default();
        let transport = UdpTransport::new(config).unwrap();
        
        // Test avec des données invalides
        let invalid_data = b"invalid packet data";
        let source_addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();
        
        let result = transport.deserialize_packet(invalid_data, source_addr);
        assert!(result.is_err());
    }
}
