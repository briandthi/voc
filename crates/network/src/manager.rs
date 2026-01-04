//! Manager réseau P2P haut niveau
//! 
//! Ce module implémente la logique métier de connexion peer-to-peer,
//! incluant les handshakes, heartbeats, et la gestion d'état de connexion.
//! Il orchestre le transport bas niveau et fournit une API simple pour l'audio.

use async_trait::async_trait;
use tokio::time::{Duration, sleep};
use std::time::Instant;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use crate::{
    NetworkManager, NetworkTransport, UdpTransport, SimulatedTransport,
    NetworkPacket, PacketType, ConnectionState, NetworkConfig, NetworkStats,
    NetworkResult, NetworkError
};
use audio::CompressedFrame;

/// Manager réseau P2P pour communication audio
/// 
/// Cette structure orchestre la communication P2P complète, de la connexion
/// initiale jusqu'à l'échange d'audio, en gérant tous les aspects de la
/// session (handshake, heartbeat, reconnexion).
/// 
/// # Architecture
/// - Transport UDP abstrait (réel ou simulé)
/// - Machine à états pour la connexion
/// - Threads séparés pour heartbeat et réception
/// - Buffer anti-jitter intégré
/// - Statistiques temps réel
/// 
/// # Example
/// ```rust,no_run
/// use network::{UdpNetworkManager, NetworkManager, NetworkConfig};
/// 
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = NetworkConfig::default();
/// let mut manager = UdpNetworkManager::new(config)?;
/// 
/// // Mode serveur
/// manager.start_listening(9001).await?;
/// 
/// // Ou mode client
/// // manager.connect_to_peer("192.168.1.100:9001".parse()?).await?;
/// # Ok(())
/// # }
/// ```
pub struct UdpNetworkManager {
    /// Configuration réseau
    config: NetworkConfig,
    
    /// Transport UDP sous-jacent
    transport: Box<dyn NetworkTransport + Send + Sync>,
    
    /// État de connexion actuel
    connection_state: Arc<Mutex<ConnectionState>>,
    
    /// ID de session unique
    session_id: u32,
    
    /// ID local unique
    sender_id: u32,
    
    /// Numéro de séquence pour les paquets envoyés
    sequence_counter: u64,
    
    /// Handle pour le thread de heartbeat
    heartbeat_handle: Option<tokio::task::JoinHandle<()>>,
    
    /// Canal pour recevoir les frames audio
    _audio_receiver: Option<mpsc::Receiver<CompressedFrame>>,
    
    /// Canal pour envoyer les frames audio
    audio_sender: Option<mpsc::Sender<CompressedFrame>>,
    
    /// Buffer anti-jitter pour réception
    receive_buffer: JitterBuffer,
    
    /// Statistiques combinées
    stats: Arc<Mutex<NetworkStats>>,
}

impl UdpNetworkManager {
    /// Crée un nouveau manager avec transport UDP réel
    /// 
    /// # Arguments
    /// * `config` - Configuration réseau
    /// 
    /// # Example
    /// ```rust
    /// use network::{UdpNetworkManager, NetworkConfig};
    /// 
    /// let config = NetworkConfig::default();
    /// let manager = UdpNetworkManager::new(config).unwrap();
    /// ```
    pub fn new(config: NetworkConfig) -> NetworkResult<Self> {
        let transport = Box::new(UdpTransport::new(config.clone())?);
        Self::with_transport(config, transport)
    }
    
    /// Crée un nouveau manager avec transport simulé pour tests
    /// 
    /// # Arguments
    /// * `config` - Configuration réseau
    /// 
    /// # Example
    /// ```rust
    /// use network::{UdpNetworkManager, NetworkConfig};
    /// 
    /// let config = NetworkConfig::test_config();
    /// let manager = UdpNetworkManager::new_simulated(config).unwrap();
    /// ```
    pub fn new_simulated(config: NetworkConfig) -> NetworkResult<Self> {
        let transport = Box::new(SimulatedTransport::new(config.clone())?);
        Self::with_transport(config, transport)
    }
    
    /// Crée un manager avec un transport personnalisé
    fn with_transport(
        config: NetworkConfig, 
        transport: Box<dyn NetworkTransport + Send + Sync>
    ) -> NetworkResult<Self> {
        let session_id = fastrand::u32(1..=u32::MAX);
        let sender_id = fastrand::u32(1..=u32::MAX);
        
        let (audio_tx, audio_rx) = mpsc::channel(config.receive_buffer_size);
        
        Ok(Self {
            config: config.clone(),
            transport,
            connection_state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
            session_id,
            sender_id,
            sequence_counter: 0,
            heartbeat_handle: None,
            _audio_receiver: Some(audio_rx),
            audio_sender: Some(audio_tx),
            receive_buffer: JitterBuffer::new(config.receive_buffer_size),
            stats: Arc::new(Mutex::new(NetworkStats::new())),
        })
    }
    
    /// Démarre le thread de heartbeat
    /// 
    /// Envoie des paquets keep-alive périodiques pour maintenir la connexion.
    async fn start_heartbeat(&mut self, _peer_addr: SocketAddr) -> NetworkResult<()> {
        if self.heartbeat_handle.is_some() {
            return Ok(()); // Déjà démarré
        }
        
        // Pour l'instant, on simplifie en ne gérant pas les heartbeats automatiques
        // Dans une version complète, on créerait un thread dédié
        
        // TODO: Implémenter le thread de heartbeat complet
        // let state_clone = self.connection_state.clone();
        // let interval_duration = self.config.heartbeat_interval;
        
        println!("Heartbeat thread started (placeholder)");
        Ok(())
    }
    
    /// Arrête le thread de heartbeat
    async fn stop_heartbeat(&mut self) {
        if let Some(handle) = self.heartbeat_handle.take() {
            handle.abort();
        }
    }
    
    /// Effectue le handshake initial avec un peer
    async fn perform_handshake(&mut self, peer_addr: SocketAddr) -> NetworkResult<()> {
        // Crée un paquet handshake en utilisant les méthodes helper
        let handshake = self.create_handshake_packet();
        
        // Envoie le handshake
        self.transport.send_packet(&handshake, peer_addr).await?;
        
        // Attend la réponse (timeout configurable)
        let timeout_duration = self.config.connection_timeout;
        let start_time = Instant::now();
        
        while start_time.elapsed() < timeout_duration {
            match self.transport.receive_packet().await {
                Ok((packet, source)) if source == peer_addr => {
                    if packet.packet_type == PacketType::Handshake {
                        // Handshake réussi
                        return Ok(());
                    }
                }
                Ok(_) => continue, // Paquet d'une autre source
                Err(NetworkError::Timeout) => {
                    // Continue à essayer
                    sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        
        Err(NetworkError::connection_timeout(peer_addr, timeout_duration.as_millis() as u32))
    }
    
    /// Met à jour l'état de connexion
    async fn set_connection_state(&self, new_state: ConnectionState) {
        let mut state = self.connection_state.lock().await;
        *state = new_state;
    }
    
    /// Traite un paquet reçu selon son type
    async fn handle_received_packet(&mut self, packet: NetworkPacket, source: SocketAddr) -> NetworkResult<()> {
        match packet.packet_type {
            PacketType::Audio => {
                // Ajoute au buffer anti-jitter
                if self.receive_buffer.push_packet(packet) {
                    // Essaie de sortir des paquets du buffer
                    while let Some(buffered_packet) = self.receive_buffer.pop_packet() {
                        if let Some(ref sender) = self.audio_sender {
                            let _ = sender.send(buffered_packet.compressed_frame).await;
                        }
                    }
                }
            }
            
            PacketType::Heartbeat => {
                // Met à jour le timestamp du dernier heartbeat
                self.update_last_heartbeat().await;
            }
            
            PacketType::Handshake => {
                // Répond au handshake
                let response = self.create_handshake_packet();
                self.transport.send_packet(&response, source).await?;
            }
            
            PacketType::Disconnect => {
                // Pair se déconnecte proprement
                self.set_connection_state(ConnectionState::Disconnected).await;
                self.stop_heartbeat().await;
            }
        }
        
        Ok(())
    }
    
    /// Met à jour le timestamp du dernier heartbeat
    async fn update_last_heartbeat(&self) {
        let mut state = self.connection_state.lock().await;
        if let ConnectionState::Connected { ref mut last_heartbeat, .. } = *state {
            *last_heartbeat = Instant::now();
        }
    }
    
    /// Vérifie si la connexion a timeout (pas de heartbeat reçu)
    async fn check_heartbeat_timeout(&self) -> bool {
        let state = self.connection_state.lock().await;
        if let ConnectionState::Connected { last_heartbeat, .. } = *state {
            last_heartbeat.elapsed() > self.config.heartbeat_timeout
        } else {
            false
        }
    }
    
    /// Crée un paquet handshake avec checksum correct
    fn create_handshake_packet(&self) -> NetworkPacket {
        let empty_frame = CompressedFrame::new(vec![], 0, Instant::now(), 0);
        let mut packet = NetworkPacket {
            protocol_version: NetworkPacket::CURRENT_PROTOCOL_VERSION,
            packet_type: PacketType::Handshake,
            sender_id: self.sender_id,
            session_id: self.session_id,
            compressed_frame: empty_frame,
            send_timestamp: Instant::now(),
            checksum: 0,
        };
        
        // CORRECTION: Calcule le checksum du paquet réel (avec le bon packet_type)
        packet.checksum = packet.calculate_checksum();
        packet
    }
    
    /// Crée un paquet disconnect avec checksum correct  
    fn create_disconnect_packet(&self) -> NetworkPacket {
        let empty_frame = CompressedFrame::new(vec![], 0, Instant::now(), 0);
        let mut packet = NetworkPacket {
            protocol_version: NetworkPacket::CURRENT_PROTOCOL_VERSION,
            packet_type: PacketType::Disconnect,
            sender_id: self.sender_id,
            session_id: self.session_id,
            compressed_frame: empty_frame,
            send_timestamp: Instant::now(),
            checksum: 0,
        };
        
        // CORRECTION: Calcule le checksum du paquet réel (avec le bon packet_type)
        packet.checksum = packet.calculate_checksum();
        packet
    }
}

#[async_trait]
impl NetworkManager for UdpNetworkManager {
    /// Démarre l'écoute en mode serveur
    async fn start_listening(&mut self, port: u16) -> NetworkResult<()> {
        // Bind le transport
        self.transport.bind(port).await?;
        
        // Met à jour l'état
        self.set_connection_state(ConnectionState::Disconnected).await;
        
        println!("En écoute sur le port {} - En attente de connexions...", port);
        
        // Boucle principale d'écoute - continue indéfiniment
        loop {
            // Attend une nouvelle connexion
            loop {
                match self.transport.receive_packet().await {
                    Ok((packet, source_addr)) => {
                        if packet.packet_type == PacketType::Handshake {
                            // Tentative de connexion détectée
                            self.set_connection_state(ConnectionState::Connecting {
                                target_addr: source_addr,
                                started_at: Instant::now(),
                                attempt_count: 1,
                            }).await;
                            
                            // Traite le handshake
                            self.handle_received_packet(packet, source_addr).await?;
                            
                            // Connexion établie
                            self.set_connection_state(ConnectionState::Connected {
                                peer_addr: source_addr,
                                session_id: self.session_id,
                                connected_at: Instant::now(),
                                last_heartbeat: Instant::now(),
                            }).await;
                            
                            // Démarre le heartbeat
                            self.start_heartbeat(source_addr).await?;
                            
                            println!("Connexion établie avec {}", source_addr);
                            break; // Sort de la boucle d'attente de connexion
                        }
                    }
                    Err(NetworkError::Timeout) => continue, // Continue à attendre
                    Err(e) => return Err(e),
                }
            }
            
            // Maintenant connecté - écoute les paquets jusqu'à déconnexion
            loop {
                match self.transport.receive_packet().await {
                    Ok((packet, source_addr)) => {
                        // Vérifie que c'est du bon peer
                        let current_peer = {
                            let state = self.connection_state.lock().await;
                            state.peer_addr()
                        };
                        
                        if Some(source_addr) == current_peer {
                            // Vérifie le type avant de traiter le paquet
                            let is_disconnect = packet.packet_type == PacketType::Disconnect;
                            
                            self.handle_received_packet(packet, source_addr).await?;
                            
                            // Si c'est un disconnect, sort de la boucle de connexion
                            if is_disconnect {
                                println!("Client {} déconnecté", source_addr);
                                break; // Sort de la boucle de connexion active
                            }
                        }
                    }
                    Err(NetworkError::Timeout) => {
                        // Vérifie si la connexion a timeout
                        if self.check_heartbeat_timeout().await {
                            println!("Timeout de connexion - retour en écoute");
                            self.set_connection_state(ConnectionState::Disconnected).await;
                            break; // Sort de la boucle de connexion active
                        }
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            
            // Connexion terminée - remet l'état à disconnected et continue à écouter
            self.set_connection_state(ConnectionState::Disconnected).await;
            self.stop_heartbeat().await;
            println!("Prêt pour une nouvelle connexion...");
        }
    }
    
    /// Se connecte à un peer distant
    async fn connect_to_peer(&mut self, peer_addr: SocketAddr) -> NetworkResult<()> {
        // Bind sur un port local aléatoire
        let local_port = fastrand::u16(10000..=60000);
        self.transport.bind(local_port).await?;
        
        // Met à jour l'état
        self.set_connection_state(ConnectionState::Connecting {
            target_addr: peer_addr,
            started_at: Instant::now(),
            attempt_count: 1,
        }).await;
        
        // Effectue le handshake
        self.perform_handshake(peer_addr).await?;
        
        // Connexion réussie
        self.set_connection_state(ConnectionState::Connected {
            peer_addr,
            session_id: self.session_id,
            connected_at: Instant::now(),
            last_heartbeat: Instant::now(),
        }).await;
        
        // Démarre le heartbeat
        self.start_heartbeat(peer_addr).await?;
        
        println!("Connecté à {}", peer_addr);
        Ok(())
    }
    
    /// Envoie une frame audio au peer connecté
    async fn send_audio(&mut self, frame: CompressedFrame) -> NetworkResult<()> {
        let peer_addr = {
            let state = self.connection_state.lock().await;
            match *state {
                ConnectionState::Connected { peer_addr, .. } => peer_addr,
                _ => return Err(NetworkError::InvalidState {
                    operation: "send_audio".to_string(),
                    current_state: "not connected".to_string(),
                }),
            }
        };
        
        // Crée le paquet avec un nouveau numéro de séquence
        self.sequence_counter += 1;
        let mut frame_with_sequence = frame;
        frame_with_sequence.sequence_number = self.sequence_counter;
        
        let packet = NetworkPacket::new_audio(
            frame_with_sequence,
            self.sender_id,
            self.session_id,
        );
        
        // Envoie le paquet
        self.transport.send_packet(&packet, peer_addr).await?;
        
        // Met à jour les statistiques
        let mut stats = self.stats.lock().await;
        stats.packets_sent += 1;
        
        Ok(())
    }
    
    /// Reçoit une frame audio du peer distant
    async fn receive_audio(&mut self) -> NetworkResult<CompressedFrame> {
        // Vérifie qu'on est connecté
        {
            let state = self.connection_state.lock().await;
            if !state.is_connected() {
                return Err(NetworkError::InvalidState {
                    operation: "receive_audio".to_string(),
                    current_state: "not connected".to_string(),
                });
            }
        }
        
        // Essaie d'abord le buffer local
        if let Some(packet) = self.receive_buffer.pop_packet() {
            return Ok(packet.compressed_frame);
        }
        
        // Sinon, reçoit du réseau
        loop {
            match self.transport.receive_packet().await {
                Ok((packet, source)) => {
                    // Vérifie que c'est du bon peer
                    let expected_peer = {
                        let state = self.connection_state.lock().await;
                        state.peer_addr()
                    };
                    
                    if Some(source) != expected_peer {
                        continue; // Paquet d'un autre peer, ignore
                    }
                    
                    // Traite le paquet
                    self.handle_received_packet(packet.clone(), source).await?;
                    
                    // Si c'est de l'audio, le retourne
                    if packet.packet_type == PacketType::Audio {
                        let mut stats = self.stats.lock().await;
                        stats.packets_received += 1;
                        return Ok(packet.compressed_frame);
                    }
                    
                    // Sinon continue à écouter
                }
                Err(NetworkError::Timeout) => {
                    // Vérifie si la connexion a timeout
                    if self.check_heartbeat_timeout().await {
                        let addr = self.connection_state.lock().await.peer_addr()
                            .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
                        return Err(NetworkError::PeerDisconnected { addr });
                    }
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// Déconnecte proprement du peer
    async fn disconnect(&mut self) -> NetworkResult<()> {
        let peer_addr = {
            let state = self.connection_state.lock().await;
            state.peer_addr()
        };
        
        if let Some(addr) = peer_addr {
            // Envoie un paquet de déconnexion
            let disconnect_packet = self.create_disconnect_packet();
            let _ = self.transport.send_packet(&disconnect_packet, addr).await;
        }
        
        // Arrête le heartbeat
        self.stop_heartbeat().await;
        
        // Met à jour l'état
        self.set_connection_state(ConnectionState::Disconnected).await;
        
        println!("Déconnexion terminée");
        Ok(())
    }
    
    /// Retourne l'état de connexion actuel
    fn connection_state(&self) -> ConnectionState {
        // Version synchrone pour éviter de bloquer
        match self.connection_state.try_lock() {
            Ok(state) => state.clone(),
            Err(_) => ConnectionState::Disconnected,
        }
    }
    
    /// Retourne les statistiques réseau combinées
    fn network_stats(&self) -> NetworkStats {
        match self.stats.try_lock() {
            Ok(stats) => stats.clone(),
            Err(_) => NetworkStats::default(),
        }
    }
    
    /// Force une reconnexion si possible
    async fn reconnect(&mut self) -> NetworkResult<()> {
        // Récupère l'adresse du peer précédent
        let peer_addr = {
            let state = self.connection_state.lock().await;
            state.peer_addr()
        };
        
        if let Some(addr) = peer_addr {
            // Déconnecte proprement d'abord
            self.disconnect().await?;
            
            // Attend un peu avant de reconnecter
            sleep(Duration::from_millis(500)).await;
            
            // Tente de reconnecter
            self.connect_to_peer(addr).await
        } else {
            Err(NetworkError::InvalidState {
                operation: "reconnect".to_string(),
                current_state: "no previous peer".to_string(),
            })
        }
    }
}

/// Buffer anti-jitter simple pour les paquets réseau
/// 
/// Compense les variations de latence réseau en buffering intelligemment
/// les paquets avant de les livrer à l'application.
struct JitterBuffer {
    /// Paquets en attente, triés par numéro de séquence
    packets: std::collections::BTreeMap<u64, NetworkPacket>,
    
    /// Taille maximum du buffer
    max_size: usize,
    
    /// Numéro de séquence attendu
    expected_sequence: u64,
    
    /// Paquets perdus détectés
    lost_packets: u64,
}

impl JitterBuffer {
    /// Crée un nouveau buffer anti-jitter
    fn new(max_size: usize) -> Self {
        Self {
            packets: std::collections::BTreeMap::new(),
            max_size,
            expected_sequence: 1,
            lost_packets: 0,
        }
    }
    
    /// Ajoute un paquet au buffer
    /// 
    /// Retourne true si le paquet a été accepté
    fn push_packet(&mut self, packet: NetworkPacket) -> bool {
        let sequence = packet.compressed_frame.sequence_number;
        
        // Rejette les paquets trop anciens ou en double
        if sequence < self.expected_sequence || self.packets.contains_key(&sequence) {
            return false;
        }
        
        // Vérifie la capacité du buffer
        if self.packets.len() >= self.max_size {
            // Supprime le plus ancien paquet
            if let Some((&oldest_seq, _)) = self.packets.iter().next() {
                self.packets.remove(&oldest_seq);
            }
        }
        
        // Ajoute le paquet
        self.packets.insert(sequence, packet);
        true
    }
    
    /// Récupère le prochain paquet dans l'ordre
    fn pop_packet(&mut self) -> Option<NetworkPacket> {
        // Cherche le paquet avec le numéro de séquence attendu
        if let Some(packet) = self.packets.remove(&self.expected_sequence) {
            self.expected_sequence += 1;
            return Some(packet);
        }
        
        // Si pas trouvé, vérifie s'il faut déclarer des paquets perdus
        let mut found_higher = false;
        for &seq in self.packets.keys() {
            if seq > self.expected_sequence {
                found_higher = true;
                break;
            }
        }
        
        if found_higher {
            // Il y a des paquets plus récents, donc celui attendu est perdu
            self.lost_packets += 1;
            self.expected_sequence += 1;
            
            // Réessaie avec le nouveau numéro attendu
            return self.pop_packet();
        }
        
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    
    #[tokio::test]
    async fn test_manager_creation() {
        let config = NetworkConfig::test_config();
        let manager = UdpNetworkManager::new_simulated(config).unwrap();
        
        assert!(!manager.connection_state().is_connected());
        assert_eq!(manager.network_stats().packets_sent, 0);
    }
    
    #[test]
    fn test_jitter_buffer() {
        let mut buffer = JitterBuffer::new(10);
        
        // Test ajout de paquets dans l'ordre
        let frame1 = CompressedFrame::new(vec![1], 960, Instant::now(), 1);
        let packet1 = NetworkPacket::new_audio(frame1, 123, 456);
        
        assert!(buffer.push_packet(packet1.clone()));
        
        // Test récupération
        let received = buffer.pop_packet().unwrap();
        assert_eq!(received.compressed_frame.sequence_number, 1);
        
        // Test paquet en retard (rejeté)
        let frame_old = CompressedFrame::new(vec![0], 960, Instant::now(), 1);
        let packet_old = NetworkPacket::new_audio(frame_old, 123, 456);
        assert!(!buffer.push_packet(packet_old));
    }
    
    #[test]
    fn test_jitter_buffer_out_of_order() {
        let mut buffer = JitterBuffer::new(10);
        
        // Ajoute des paquets dans le désordre
        let frame3 = CompressedFrame::new(vec![3], 960, Instant::now(), 3);
        let packet3 = NetworkPacket::new_audio(frame3, 123, 456);
        assert!(buffer.push_packet(packet3));
        
        let frame1 = CompressedFrame::new(vec![1], 960, Instant::now(), 1);
        let packet1 = NetworkPacket::new_audio(frame1, 123, 456);
        assert!(buffer.push_packet(packet1));
        
        // Le paquet 1 doit sortir en premier
        let received = buffer.pop_packet().unwrap();
        assert_eq!(received.compressed_frame.sequence_number, 1);
        
        // Le paquet 2 est manquant, doit être marqué comme perdu
        // et le paquet 3 doit sortir
        let received = buffer.pop_packet().unwrap();
        assert_eq!(received.compressed_frame.sequence_number, 3);
        assert_eq!(buffer.lost_packets, 1);
    }
}
