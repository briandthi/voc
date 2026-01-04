// Client simple pour tests P2P Voc
// 
// Cette application fournit un client basique pour tester
// la communication P2P entre deux instances.

use std::io::{self, Write};
use std::time::Duration;

use clap::{Parser, Subcommand};
use tokio::signal;
use network::{
    NetworkConfig, UdpNetworkManager, NetworkManager, 
    utils, NetworkResult
};
use audio::CompressedFrame;

#[derive(Parser)]
#[command(author, version, about = "Client simple Voc pour tests P2P")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Lance un serveur d'écoute
    Listen {
        #[arg(short, long, default_value = "9001")]
        port: u16,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Se connecte à un serveur
    Connect {
        #[arg(short, long)]
        server: String,
        #[arg(short, long)]
        verbose: bool,
        #[arg(short, long, default_value = "10")]
        frames: u32,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Listen { port, verbose } => {
            run_server(port, verbose).await?
        },
        Commands::Connect { server, verbose, frames } => {
            run_client(&server, verbose, frames).await?
        },
    }
    
    Ok(())
}

/// Lance un serveur d'écoute
async fn run_server(port: u16, verbose: bool) -> NetworkResult<()> {
    let config = NetworkConfig::lan_optimized();
    let mut manager = UdpNetworkManager::new(config)?;
    
    println!("🚀 Démarrage serveur Voc sur port {}...", port);
    
    manager.start_listening(port).await?;
    
    if let Ok(local_ip) = utils::get_local_ip() {
        println!("✅ Serveur prêt !");
        println!("📡 Connexion possible via :");
        println!("   🌍 {}:{}", local_ip, port);
        println!("   🏠 127.0.0.1:{}", port);
    } else {
        println!("✅ Serveur prêt sur port {} !", port);
    }
    
    println!("\n📋 Utilisation :");
    println!("   • Autres instances : cargo run --bin voc-client connect --server IP:PORT");
    println!("   • Arrêt : Ctrl+C");
    
    if verbose {
        println!("\n🔍 Mode verbose activé - affichage des détails");
    }
    
    // Boucle d'écoute avec gestion des signaux
    println!("\n⏳ En attente de connexions...");
    
    // Utilise tokio::select pour gérer les signaux et autres événements
    tokio::select! {
        // Gestion du signal Ctrl+C
        _ = signal::ctrl_c() => {
            println!("\n🛑 Arrêt du serveur demandé");
        }
        
        // Simulation d'écoute continue (dans une vraie implémentation,
        // on aurait une boucle qui gère les connexions entrantes)
        _ = tokio::time::sleep(Duration::from_secs(3600)) => {
            // Timeout après 1h
            println!("\n⏰ Timeout serveur (1h)");
        }
    }
    
    println!("🔌 Fermeture du serveur...");
    manager.disconnect().await?;
    println!("👋 Serveur arrêté");
    
    Ok(())
}

/// Lance un client et se connecte au serveur
async fn run_client(server_str: &str, verbose: bool, frame_count: u32) -> NetworkResult<()> {
    let server_addr = utils::parse_address(server_str)?;
    
    let config = NetworkConfig::lan_optimized();
    let mut manager = UdpNetworkManager::new(config)?;
    
    println!("🚀 Client Voc");
    println!("📡 Connexion au serveur {}...", server_addr);
    
    if verbose {
        println!("🔍 Mode verbose activé");
    }
    
    // Tentative de connexion
    match manager.connect_to_peer(server_addr).await {
        Ok(()) => {
            println!("✅ Connexion établie avec succès !");
            
            // Test d'envoi de frames audio
            println!("📤 Envoi de {} frames de test...", frame_count);
            
            let mut successful_sends = 0;
            let mut failed_sends = 0;
            
            for i in 0..frame_count {
                let frame = create_test_audio_frame(i);
                
                match manager.send_audio(frame).await {
                    Ok(()) => {
                        successful_sends += 1;
                        if verbose {
                            println!("   📤 Frame {} envoyée ✅", i);
                        } else if i % 10 == 0 {
                            print!(".");
                            io::stdout().flush().unwrap();
                        }
                    },
                    Err(e) => {
                        failed_sends += 1;
                        if verbose {
                            println!("   ❌ Frame {} échouée : {}", i, e);
                        }
                    }
                }
                
                // Pause inter-frames (simulation audio temps réel)
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            
            if !verbose {
                println!(); // Nouvelle ligne après les points
            }
            
            // Résultats
            println!("\n📈 Résultats :");
            println!("   ✅ Frames envoyées : {}", successful_sends);
            if failed_sends > 0 {
                println!("   ❌ Échecs : {}", failed_sends);
            }
            println!("   📊 Taux de succès : {:.1}%", 
                     (successful_sends as f32 / frame_count as f32) * 100.0);
            
            // Test de réception (optionnel)
            if verbose {
                println!("\n📥 Test réception (5s)...");
                let start = std::time::Instant::now();
                let mut received_count = 0;
                
                while start.elapsed() < Duration::from_secs(5) {
                    match tokio::time::timeout(
                        Duration::from_millis(100), 
                        manager.receive_audio()
                    ).await {
                        Ok(Ok(_frame)) => {
                            received_count += 1;
                            println!("   📥 Frame reçue #{}", received_count);
                        },
                        Ok(Err(_)) => {
                            // Erreur de réception (normal s'il n'y a rien à recevoir)
                        },
                        Err(_) => {
                            // Timeout (normal)
                        }
                    }
                }
                
                if received_count > 0 {
                    println!("   📊 Total reçu : {} frames", received_count);
                } else {
                    println!("   ℹ️  Aucune frame reçue (normal en test unidirectionnel)");
                }
            }
            
            println!("✅ Test terminé avec succès");
        },
        Err(e) => {
            println!("❌ Échec de connexion : {}", e);
            return Err(e);
        }
    }
    
    // Déconnexion propre
    println!("🔌 Déconnexion...");
    manager.disconnect().await?;
    println!("👋 Client fermé");
    
    Ok(())
}

/// Crée une frame audio de test
fn create_test_audio_frame(sequence: u32) -> CompressedFrame {
    use std::time::Instant;
    
    // Simulation de données audio Opus compressées
    let mut audio_data = vec![0u8; 180]; // ~180 bytes typique pour Opus VoIP
    
    // Pattern de données pseudo-aléatoires basé sur la séquence
    for (i, byte) in audio_data.iter_mut().enumerate() {
        *byte = ((sequence * 73 + i as u32 * 31) & 0xFF) as u8;
    }
    
    // Ajoute un header simulé Opus
    audio_data[0] = 0xF8; // Configuration Opus typique
    audio_data[1] = (sequence & 0xFF) as u8; // Numéro de frame dans le header
    
    CompressedFrame::new(
        audio_data,
        960, // 20ms à 48kHz = 960 échantillons
        Instant::now(),
        sequence as u64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_audio_frame_creation() {
        let frame = create_test_audio_frame(42);
        
        assert_eq!(frame.data.len(), 180);
        assert_eq!(frame.original_sample_count, 960);
        assert_eq!(frame.sequence_number, 42);
        
        // Vérifie le header simulé
        assert_eq!(frame.data[0], 0xF8);
        assert_eq!(frame.data[1], 42);
    }
    
    #[test] 
    fn test_different_sequences() {
        let frame1 = create_test_audio_frame(0);
        let frame2 = create_test_audio_frame(1);
        
        // Les frames doivent être différentes
        assert_ne!(frame1.data, frame2.data);
        assert_ne!(frame1.sequence_number, frame2.sequence_number);
    }
}
