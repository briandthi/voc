//! Crate audio pour Voc - Communication vocale temps réel
//! 
//! Ce crate gère toute la chaîne audio :
//! - Capture microphone avec cpal
//! - Compression/décompression Opus
//! - Lecture audio avec cpal
//! - Pipeline de test complet

pub mod config;      // Configuration audio
pub mod types;       // Types de données (AudioFrame, etc.)
pub mod traits;      // Traits abstraits
pub mod capture;     // Implémentation capture avec cpal
pub mod playback;    // Implémentation lecture avec cpal
pub mod codec;       // Implémentation Opus
pub mod pipeline;    // Pipeline de test
pub mod error;       // Gestion d'erreurs

// Réexports pour faciliter l'utilisation
pub use config::*;
pub use types::*;
pub use traits::*;
pub use error::*;

// Réexports des implémentations principales
pub use capture::CpalCapture;
pub use playback::CpalPlayback;
pub use codec::OpusCodec;
pub use pipeline::AudioPipelineImpl;
