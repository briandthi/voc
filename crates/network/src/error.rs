//! Gestion d'erreurs pour le système networking
//! 
//! Ce module définit tous les types d'erreurs possibles dans notre système réseau P2P.
//! Il suit les mêmes patterns que le module audio pour la cohérence du code.

use thiserror::Error;
use std::net::SocketAddr;

/// Énumération de toutes les erreurs possibles dans le système réseau
/// 
/// `thiserror::Error` génère automatiquement l'implémentation du trait Error
/// avec des messages d'erreur descriptifs en français.
#[derive(Error, Debug)]
pub enum NetworkError {
    /// Impossible de créer ou bind le socket UDP sur le port demandé
    #[error("Impossible de bind le socket sur le port {port}: {reason}")]
    BindError { port: u16, reason: String },
    
    /// Timeout lors de la tentative de connexion vers un peer
    #[error("Timeout de connexion vers {addr} après {timeout_ms}ms")]
    ConnectionTimeout { addr: SocketAddr, timeout_ms: u32 },
    
    /// Le peer distant s'est déconnecté de façon inattendue
    #[error("Peer {addr} déconnecté de façon inattendue")]
    PeerDisconnected { addr: SocketAddr },
    
    /// Paquet reçu avec un checksum invalide (corruption réseau)
    #[error("Paquet corrompu reçu de {addr}: checksum invalide")]
    CorruptedPacket { addr: SocketAddr },
    
    /// Paquet trop volumineux pour le MTU réseau
    #[error("Paquet trop volumineux: {size} bytes (max autorisé: {max} bytes)")]
    PacketTooLarge { size: usize, max: usize },
    
    /// Paquet reçu avec un format invalide ou version incompatible
    #[error("Format de paquet invalide reçu de {addr}")]
    InvalidPacketFormat { addr: SocketAddr },
    
    /// Session ID mismatch - paquet d'une ancienne session
    #[error("Session ID invalide: reçu {received}, attendu {expected}")]
    InvalidSessionId { received: u32, expected: u32 },
    
    /// Numéro de séquence trop ancien (paquet en retard)
    #[error("Paquet en retard: séquence {sequence}, retard de {age_ms}ms")]
    PacketTooOld { sequence: u64, age_ms: u64 },
    
    /// Buffer réseau plein, impossible d'accepter plus de paquets
    #[error("Buffer réseau plein ({capacity} paquets), paquet dropé")]
    BufferOverflow { capacity: usize },
    
    /// Aucune donnée disponible dans le buffer de réception
    #[error("Buffer réseau vide, aucune donnée à lire")]
    BufferUnderflow,
    
    /// Timeout lors d'une opération réseau
    #[error("Timeout - aucune réponse reçue dans le délai imparti")]
    Timeout,
    
    /// Adresse IP ou port invalide fourni par l'utilisateur
    #[error("Adresse invalide: {addr}")]
    InvalidAddress { addr: String },
    
    /// Erreur lors de la sérialisation/désérialisation des paquets
    #[error("Erreur de sérialisation: {0}")]
    SerializationError(#[from] bincode::Error),
    
    /// Erreur générale d'entrée/sortie réseau
    #[error("Erreur IO réseau: {0}")]
    IoError(#[from] std::io::Error),
    
    /// Erreur lors de l'initialisation des composants réseau
    #[error("Erreur d'initialisation réseau: {0}")]
    InitializationError(String),
    
    /// Opération tentée alors que la connexion n'est pas dans le bon état
    #[error("Opération {operation} invalide dans l'état {current_state}")]
    InvalidState { operation: String, current_state: String },
    
    /// Erreur de configuration réseau
    #[error("Configuration réseau invalide: {0}")]
    ConfigError(String),
}

/// Conversion automatique des erreurs de parsing d'adresses
impl From<std::net::AddrParseError> for NetworkError {
    fn from(err: std::net::AddrParseError) -> Self {
        NetworkError::InvalidAddress { 
            addr: format!("Erreur de parsing: {}", err) 
        }
    }
}

/// Type Result personnalisé pour notre crate network
/// 
/// Au lieu d'écrire Result<T, NetworkError> partout, on peut écrire NetworkResult<T>
pub type NetworkResult<T> = Result<T, NetworkError>;

/// Fonctions utilitaires pour créer des erreurs communes
impl NetworkError {
    /// Crée une erreur de bind avec contexte
    pub fn bind_failed(port: u16, cause: std::io::Error) -> Self {
        Self::BindError {
            port,
            reason: cause.to_string(),
        }
    }
    
    /// Crée une erreur de timeout avec contexte
    pub fn connection_timeout(addr: SocketAddr, timeout_ms: u32) -> Self {
        Self::ConnectionTimeout { addr, timeout_ms }
    }
    
    /// Crée une erreur de paquet corrompu
    pub fn corrupted_packet(addr: SocketAddr) -> Self {
        Self::CorruptedPacket { addr }
    }
    
    /// Crée une erreur de paquet trop volumineux
    pub fn packet_too_large(size: usize, max: usize) -> Self {
        Self::PacketTooLarge { size, max }
    }
    
    /// Vérifie si l'erreur est récupérable (worth retrying)
    pub fn is_recoverable(&self) -> bool {
        match self {
            NetworkError::ConnectionTimeout { .. } => true,
            NetworkError::BufferOverflow { .. } => true,
            NetworkError::BufferUnderflow => true,
            NetworkError::PacketTooOld { .. } => true,
            NetworkError::CorruptedPacket { .. } => true,
            _ => false,
        }
    }
    
    /// Vérifie si l'erreur nécessite une reconnexion
    pub fn requires_reconnection(&self) -> bool {
        match self {
            NetworkError::PeerDisconnected { .. } => true,
            NetworkError::InvalidSessionId { .. } => true,
            NetworkError::ConnectionTimeout { .. } => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_display() {
        let error = NetworkError::BindError { 
            port: 9001, 
            reason: "Port déjà utilisé".to_string() 
        };
        assert!(error.to_string().contains("9001"));
        assert!(error.to_string().contains("Port déjà utilisé"));
    }
    
    #[test]
    fn test_error_recoverable() {
        let timeout_error = NetworkError::ConnectionTimeout { 
            addr: "127.0.0.1:9001".parse().unwrap(),
            timeout_ms: 5000 
        };
        assert!(timeout_error.is_recoverable());
        
        let bind_error = NetworkError::BindError { 
            port: 9001, 
            reason: "Permission refusée".to_string() 
        };
        assert!(!bind_error.is_recoverable());
    }
    
    #[test]
    fn test_error_requires_reconnection() {
        let disconnected = NetworkError::PeerDisconnected { 
            addr: "127.0.0.1:9001".parse().unwrap() 
        };
        assert!(disconnected.requires_reconnection());
        
        let buffer_overflow = NetworkError::BufferOverflow { capacity: 100 };
        assert!(!buffer_overflow.requires_reconnection());
    }
    
    #[test]
    fn test_helper_functions() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "test");
        let error = NetworkError::bind_failed(8080, io_err);
        
        match error {
            NetworkError::BindError { port, reason } => {
                assert_eq!(port, 8080);
                assert!(reason.contains("test"));
            }
            _ => panic!("Wrong error type"),
        }
    }
}
