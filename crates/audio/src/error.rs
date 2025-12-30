//! Gestion d'erreurs pour le système audio
//! 
//! Ce module définit tous les types d'erreurs possibles dans notre système audio.
//! En Rust, nous utilisons le type Result<T, E> pour gérer les erreurs de façon explicite.

use thiserror::Error;

/// Énumération de toutes les erreurs possibles dans le système audio
/// 
/// `thiserror::Error` génère automatiquement l'implémentation du trait Error
/// et nous permet de définir des messages d'erreur avec `#[error("...")]`
#[derive(Error, Debug)]
pub enum AudioError {
    /// Aucun périphérique audio (microphone ou haut-parleurs) n'a été trouvé
    #[error("Aucun périphérique audio trouvé")]
    NoDeviceFound,
    
    /// Erreur lors de la configuration des paramètres audio (sample rate, etc.)
    #[error("Erreur de configuration audio: {0}")]
    ConfigError(String),
    
    /// Erreur provenant de la librairie cpal (Cross-Platform Audio Library)
    /// `#[from]` génère automatiquement une conversion depuis l'erreur cpal
    #[error("Erreur cpal: {0}")]
    CpalError(#[from] cpal::PlayStreamError),
    
    /// Erreur lors de l'encodage/décodage Opus
    #[error("Erreur Opus: {0}")]
    OpusError(String),
    
    /// Le buffer audio est plein - on doit dropper des frames
    #[error("Buffer overflow - frame perdue")]
    BufferOverflow,
    
    /// Le buffer audio est vide - pas de données à jouer
    #[error("Buffer underrun - pas de données")]
    BufferUnderrun,
    
    /// Une opération a pris trop de temps (timeout)
    #[error("Timeout - opération trop lente")]
    Timeout,
    
    /// Le périphérique audio a été débranché pendant l'utilisation
    #[error("Périphérique audio déconnecté")]
    DeviceDisconnected,
    
    /// Erreur lors de l'initialisation d'un composant
    #[error("Erreur d'initialisation: {0}")]
    InitializationError(String),
}

/// Conversion automatique des erreurs Opus vers AudioError
/// 
/// Cela nous permet d'utiliser l'opérateur `?` avec les fonctions Opus
impl From<opus::Error> for AudioError {
    fn from(err: opus::Error) -> Self {
        AudioError::OpusError(format!("{:?}", err))
    }
}

/// Conversion des erreurs cpal::BuildStreamError
impl From<cpal::BuildStreamError> for AudioError {
    fn from(err: cpal::BuildStreamError) -> Self {
        AudioError::ConfigError(format!("Erreur construction stream: {:?}", err))
    }
}

/// Conversion des erreurs cpal::DefaultStreamConfigError
impl From<cpal::DefaultStreamConfigError> for AudioError {
    fn from(err: cpal::DefaultStreamConfigError) -> Self {
        AudioError::ConfigError(format!("Erreur config par défaut: {:?}", err))
    }
}

/// Conversion des erreurs cpal::PauseStreamError
impl From<cpal::PauseStreamError> for AudioError {
    fn from(err: cpal::PauseStreamError) -> Self {
        AudioError::ConfigError(format!("Erreur pause stream: {:?}", err))
    }
}

/// Type Result personnalisé pour notre crate
/// 
/// Au lieu d'écrire Result<T, AudioError> partout, on peut écrire AudioResult<T>
pub type AudioResult<T> = Result<T, AudioError>;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_display() {
        // Test que nos messages d'erreurs s'affichent correctement
        let error = AudioError::NoDeviceFound;
        assert_eq!(error.to_string(), "Aucun périphérique audio trouvé");
        
        let error = AudioError::ConfigError("Test".to_string());
        assert_eq!(error.to_string(), "Erreur de configuration audio: Test");
    }
}
