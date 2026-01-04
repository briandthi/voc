// Application de test pour le système réseau Voc
// 
// Cette application permet de tester tous les composants réseau :
// - Test du transport UDP
// - Test des connexions P2P
// - Test de latence et performance
// - Simulation de conditions réseau

use std::io::{self, Write};
use std::time::{Duration, Instant};
use std::net::SocketAddr;

use clap::{Parser, Subcommand};
use network::{
    NetworkConfig, UdpNetworkManager, NetworkManager, NetworkTransport,
    UdpTransport, SimulatedTransport, NetworkStats, ConnectionState,
    utils, NetworkResult, NetworkError, NetworkPacket, PacketType
};
use audio::{CompressedFrame};

#[derive(Parser)]
#[command(author, version, about = "Application de test réseau Voc")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Test interactif complet
    Interactive,
    /// Test de transport UDP basique
    Transport {
        #[arg(short, long, default_value = "9001")]
        port: u16,
    },
    /// Test loopback (simulation)
    Loopback {
        #[arg(short, long, default_value = "30")]
        duration: u32,
        #[arg(long, default_value = "0")]
        latency: u32,
        #[arg(long, default_value = "0.0")]
        loss: f32,
    },
    /// Test performance réseau
    Performance {
        #[arg(short, long, default_value = "60")]
        duration: u32,
        #[arg(short, long, default_value = "9001")]
        port: u16,
    },
    /// Client pour test P2P
    Client {
        #[arg(short, long)]
        server: String,
    },
    /// Serveur pour test P2P
    Server {
        #[arg(short, long, default_value = "9001")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    match &cli.command {
        Some(Commands::Interactive) => run_interactive().await?,
        Some(Commands::Transport { port }) => test_transport(*port).await?,
        Some(Commands::Loopback { duration, latency, loss }) => {
            test_loopback(*duration, *latency, *loss).await?
        },
        Some(Commands::Performance { duration, port }) => {
            test_performance(*duration, *port).await?
        },
        Some(Commands::Client { server }) => {
            run_client(server).await?
        },
        Some(Commands::Server { port }) => {
            run_server(*port).await?
        },
        None => run_interactive().await?,
    }
    
    Ok(())
}

/// Mode interactif principal
async fn run_interactive() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 Application de test réseau Voc");
    println!("==================================");
    
    // Test de la configuration
    println!("\n1️⃣  Test de la configuration...");
    test_config()?;
    
    // Test des utilitaires
    println!("\n2️⃣  Test des utilitaires...");
    test_utilities()?;
    
    // Menu interactif
    loop {
        println!("\n🎛️  Menu principal :");
        println!("   1 - Test transport UDP");
        println!("   2 - Test loopback simulé");
        println!("   3 - Test performance");
        println!("   4 - Test P2P (serveur)");
        println!("   5 - Test P2P (client)");
        println!("   6 - Informations réseau");
        println!("   q - Quitter");
        
        print!("Votre choix : ");
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        
        match input.trim() {
            "1" => interactive_transport_test().await?,
            "2" => interactive_loopback_test().await?,
            "3" => interactive_performance_test().await?,
            "4" => interactive_server_test().await?,
            "5" => interactive_client_test().await?,
            "6" => show_network_info().await?,
            "q" | "Q" => break,
            _ => println!("❌ Choix invalide"),
        }
    }
    
    println!("👋 Au revoir !");
    Ok(())
}

/// Test de la configuration réseau
fn test_config() -> Result<(), Box<dyn std::error::Error>> {
    let config = NetworkConfig::default();
    
    println!("✅ Configuration par défaut :");
    println!("   Port local : {}", config.local_port);
    println!("   Taille buffer socket : {} bytes", utils::format_bytes(config.socket_buffer_size));
    println!("   Taille buffer réception : {} paquets", config.receive_buffer_size);
    println!("   Timeout connexion : {}", utils::format_duration(config.connection_timeout));
    println!("   Taille max paquet : {} bytes", NetworkPacket::MAX_PACKET_SIZE);
    println!("   Intervalle heartbeat : {}", utils::format_duration(config.heartbeat_interval));
    println!("   Âge max paquet : {}", utils::format_duration(config.max_packet_age));
    
    // Test des presets
    let lan_config = NetworkConfig::lan_optimized();
    let wan_config = NetworkConfig::wan_optimized();
    let test_config = NetworkConfig::test_config();
    
    println!("\n📊 Comparaison des presets :");
    println!("   LAN - Timeout: {}, Heartbeat: {}", 
             utils::format_duration(lan_config.connection_timeout),
             utils::format_duration(lan_config.heartbeat_interval));
    println!("   WAN - Timeout: {}, Heartbeat: {}", 
             utils::format_duration(wan_config.connection_timeout),
             utils::format_duration(wan_config.heartbeat_interval));
    println!("   TEST - Timeout: {}, Retry: {}", 
             utils::format_duration(test_config.connection_timeout),
             test_config.max_retry_attempts);
    
    Ok(())
}

/// Test des fonctions utilitaires
fn test_utilities() -> Result<(), Box<dyn std::error::Error>> {
    // Test parsing d'adresses
    let test_addresses = [
        "127.0.0.1:9001",
        "192.168.1.100:8080",
        "10.0.0.1:3000",
    ];
    
    println!("✅ Test parsing d'adresses :");
    for addr_str in &test_addresses {
        match utils::parse_address(addr_str) {
            Ok(addr) => println!("   {} → {}", addr_str, addr),
            Err(e) => println!("   {} → ❌ {}", addr_str, e),
        }
    }
    
    // Test localhost
    let localhost = utils::localhost(9001);
    println!("   localhost(9001) → {}", localhost);
    
    // Test IP locale
    match utils::get_local_ip() {
        Ok(ip) => println!("   IP locale : {}", ip),
        Err(e) => println!("   ⚠️  Impossible de détecter l'IP locale : {}", e),
    }
    
    // Test formatage
    println!("\n📏 Test formatage :");
    println!("   {} → {}", 1234, utils::format_bytes(1234));
    println!("   {} ms → {}", 1500, utils::format_duration(Duration::from_millis(1500)));
    
    Ok(())
}

/// Test transport UDP interactif
async fn interactive_transport_test() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🚛 Test Transport UDP");
    println!("====================");
    
    print!("Port d'écoute (défaut: 9001) : ");
    io::stdout().flush().unwrap();
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let port: u16 = input.trim().parse().unwrap_or(9001);
    
    test_transport(port).await
}

/// Test transport UDP sur un port donné
async fn test_transport(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let config = NetworkConfig::default();
    let mut transport = UdpTransport::new(config)?;
    
    println!("🔧 Test création transport... ✅");
    
    // Test bind
    print!("🔌 Test bind sur port {}... ", port);
    match transport.bind(port).await {
        Ok(()) => {
            println!("✅");
            if let Some(addr) = transport.local_addr() {
                println!("   Adresse locale : {}", addr);
            }
        },
        Err(e) => {
            println!("❌ {}", e);
            return Err(e.into());
        }
    }
    
    // Test état
    println!("📊 État transport : {}", if transport.is_active() { "Actif ✅" } else { "Inactif ❌" });
    
    // Test statistiques
    let stats = transport.stats();
    println!("📈 Statistiques initiales :");
    println!("   Paquets envoyés : {}", stats.packets_sent);
    println!("   Paquets reçus : {}", stats.packets_received);
    
    // Test shutdown
    print!("🛑 Test arrêt... ");
    transport.shutdown().await?;
    println!("✅");
    
    println!("📊 État final : {}", if transport.is_active() { "Actif ❌" } else { "Inactif ✅" });
    
    Ok(())
}

/// Test loopback simulé interactif
async fn interactive_loopback_test() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔄 Test Loopback Simulé");
    println!("========================");
    
    print!("Durée du test (secondes, 1-300) : ");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let duration: u32 = input.trim().parse().unwrap_or(30).clamp(1, 300);
    
    print!("Latence simulée (ms, 0-500) : ");
    io::stdout().flush().unwrap();
    input.clear();
    io::stdin().read_line(&mut input).unwrap();
    let latency: u32 = input.trim().parse().unwrap_or(0).clamp(0, 500);
    
    print!("Taux de perte (%, 0.0-50.0) : ");
    io::stdout().flush().unwrap();
    input.clear();
    io::stdin().read_line(&mut input).unwrap();
    let loss: f32 = input.trim().parse().unwrap_or(0.0_f32).clamp(0.0, 50.0);
    
    test_loopback(duration, latency, loss).await
}

/// Test loopback avec simulation réseau
async fn test_loopback(duration: u32, latency_ms: u32, loss_rate: f32) -> Result<(), Box<dyn std::error::Error>> {
    let config = NetworkConfig::test_config();
    let mut transport = SimulatedTransport::new(config)?;
    
    // Configuration simulation
    transport.set_simulation_params(latency_ms, loss_rate / 100.0, latency_ms / 4);
    
    println!("🚀 Démarrage test loopback pour {}s...", duration);
    println!("📊 Paramètres : latence={}ms, perte={:.1}%", latency_ms, loss_rate);
    
    // Bind
    transport.bind(9001).await?;
    
    let start = Instant::now();
    let mut packets_sent = 0;
    let mut packets_received = 0;
    
    // Boucle de test
    while start.elapsed().as_secs() < duration as u64 {
        // Crée et envoie un paquet test
        let frame = create_test_frame(packets_sent as u32);
        let packet = NetworkPacket::new_audio(frame, 12345, packets_sent as u32);
        
        // Envoie vers soi-même
        let target_addr = utils::localhost(9001);
        
        match transport.send_packet(&packet, target_addr).await {
            Ok(()) => packets_sent += 1,
            Err(e) => println!("⚠️  Erreur envoi : {}", e),
        }
        
        // Essaye de recevoir (non-bloquant avec timeout court)
        match tokio::time::timeout(Duration::from_millis(10), transport.receive_packet()).await {
            Ok(Ok((_received_packet, _source_addr))) => {
                packets_received += 1;
            },
            Ok(Err(_)) => {}, // Erreur réception (normal en simulation)
            Err(_) => {}, // Timeout (normal)
        }
        
        // Pause entre les paquets
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        // Affichage progressif
        if packets_sent % 50 == 0 {
            println!("📊 Envoyés: {}, Reçus: {}, Perte: {:.1}%", 
                     packets_sent, packets_received, 
                     (packets_sent - packets_received) as f32 / packets_sent as f32 * 100.0);
        }
    }
    
    // Statistiques finales
    let stats = transport.stats();
    println!("\n📈 Résultats finaux :");
    println!("   Durée : {}", utils::format_duration(start.elapsed()));
    println!("   Paquets envoyés : {}", stats.packets_sent);
    println!("   Paquets reçus : {}", stats.packets_received);
    println!("   Paquets perdus : {}", stats.packets_lost);
    println!("   Taux de perte : {:.2}%", stats.loss_percentage());
    
    transport.shutdown().await?;
    
    Ok(())
}

/// Test de performance interactif
async fn interactive_performance_test() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⚡ Test Performance");
    println!("==================");
    
    print!("Durée du test (secondes, 10-300) : ");
    io::stdout().flush().unwrap();
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let duration: u32 = input.trim().parse().unwrap_or(60).clamp(10, 300);
    
    print!("Port (défaut: 9002) : ");
    io::stdout().flush().unwrap();
    input.clear();
    io::stdin().read_line(&mut input).unwrap();
    let port: u16 = input.trim().parse().unwrap_or(9002);
    
    test_performance(duration, port).await
}

/// Test de performance réseau
async fn test_performance(duration: u32, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let config = NetworkConfig::lan_optimized();
    let mut manager = UdpNetworkManager::new(config)?;
    
    println!("🚀 Test performance pour {}s sur port {}...", duration, port);
    
    // Démarrage serveur
    manager.start_listening(port).await?;
    
    println!("✅ Manager en écoute");
    
    let start = Instant::now();
    let mut total_frames = 0;
    let mut total_bytes = 0;
    
    // Simulation envoi audio continu
    while start.elapsed().as_secs() < duration as u64 {
        let frame = create_test_frame(total_frames);
        total_bytes += frame.data.len();
        
        // Dans un vrai test, on enverrait vers un peer connecté
        // Ici on simule juste la création et validation des frames
        
        total_frames += 1;
        
        // Simulation intervalle audio (20ms par frame)
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        if total_frames % 50 == 0 {
            let elapsed = start.elapsed().as_secs_f32();
            let fps = total_frames as f32 / elapsed;
            let bps = total_bytes as f32 / elapsed;
            
            println!("📊 {} frames, {:.1} fps, {} bps", 
                     total_frames, fps, utils::format_bytes(bps as usize));
        }
    }
    
    // Résultats finaux
    let elapsed = start.elapsed();
    println!("\n📈 Performance finale :");
    println!("   Durée : {}", utils::format_duration(elapsed));
    println!("   Frames traitées : {}", total_frames);
    println!("   Débit moyen : {:.1} fps", total_frames as f32 / elapsed.as_secs_f32());
    println!("   Données : {}/s", utils::format_bytes(
        (total_bytes as f32 / elapsed.as_secs_f32()) as usize
    ));
    
    manager.disconnect().await?;
    
    Ok(())
}

/// Test serveur P2P interactif
async fn interactive_server_test() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🖥️  Test Serveur P2P");
    println!("===================");
    
    print!("Port d'écoute (défaut: 9001) : ");
    io::stdout().flush().unwrap();
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let port: u16 = input.trim().parse().unwrap_or(9001);
    
    run_server(port).await
}

/// Lance un serveur P2P
async fn run_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let config = NetworkConfig::lan_optimized();
    let mut manager = UdpNetworkManager::new(config)?;
    
    println!("🚀 Démarrage serveur sur port {}...", port);
    
    manager.start_listening(port).await?;
    
    if let Ok(local_ip) = utils::get_local_ip() {
        println!("✅ Serveur prêt ! Les clients peuvent se connecter à :");
        println!("   🌍 {}:{}", local_ip, port);
        println!("   🏠 127.0.0.1:{}", port);
    } else {
        println!("✅ Serveur prêt sur port {} !", port);
    }
    
    println!("\n📋 Commandes :");
    println!("   Ctrl+C pour arrêter le serveur");
    
    // Boucle d'écoute (simplifiée pour le test)
    println!("⏳ En attente de connexions...");
    
    // Dans une vraie implémentation, on attendrait des connexions
    // Pour ce test, on simule juste une attente
    tokio::time::sleep(Duration::from_secs(300)).await;
    
    Ok(())
}

/// Test client P2P interactif
async fn interactive_client_test() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n💻 Test Client P2P");
    println!("==================");
    
    print!("Adresse du serveur (IP:PORT) : ");
    io::stdout().flush().unwrap();
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let server_addr = input.trim().to_string();
    
    run_client(&server_addr).await
}

/// Lance un client P2P
async fn run_client(server_str: &str) -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = utils::parse_address(server_str)?;
    
    let config = NetworkConfig::lan_optimized();
    let mut manager = UdpNetworkManager::new(config)?;
    
    println!("🚀 Connexion au serveur {}...", server_addr);
    
    match manager.connect_to_peer(server_addr).await {
        Ok(()) => {
            println!("✅ Connecté avec succès !");
            
            // Test envoi de quelques frames
            for i in 0..10 {
                let frame = create_test_frame(i);
                
                match manager.send_audio(frame).await {
                    Ok(()) => println!("📤 Frame {} envoyée", i),
                    Err(e) => println!("❌ Erreur envoi frame {} : {}", i, e),
                }
                
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            
            println!("✅ Test envoi terminé");
        },
        Err(e) => {
            println!("❌ Échec connexion : {}", e);
        }
    }
    
    manager.disconnect().await?;
    
    Ok(())
}

/// Affiche les informations réseau système
async fn show_network_info() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🌐 Informations Réseau");
    println!("======================");
    
    // IP locale
    match utils::get_local_ip() {
        Ok(ip) => println!("🏠 IP locale : {}", ip),
        Err(e) => println!("⚠️  IP locale : Erreur ({})", e),
    }
    
    // Configuration par défaut
    let config = NetworkConfig::default();
    println!("\n⚙️  Configuration par défaut :");
    println!("   Port : {}", config.local_port);
    println!("   Buffer socket : {}", utils::format_bytes(config.socket_buffer_size));
    println!("   Buffer réception : {} paquets", config.receive_buffer_size);
    println!("   Timeout : {}", utils::format_duration(config.connection_timeout));
    
    // Test de connectivité localhost
    println!("\n🔗 Test connectivité localhost :");
    let test_ports = [9001, 8080, 3000];
    
    for port in test_ports {
        let addr = utils::localhost(port);
        match tokio::net::TcpListener::bind(addr).await {
            Ok(_) => println!("   Port {} : ✅ Disponible", port),
            Err(_) => println!("   Port {} : ❌ Occupé", port),
        }
    }
    
    // Statistiques système
    println!("\n💻 Système :");
    println!("   Threads CPU : {}", num_cpus::get());
    println!("   Taille SocketAddr : {} bytes", std::mem::size_of::<SocketAddr>());
    
    Ok(())
}

/// Crée une frame de test avec des données simulées
fn create_test_frame(sequence: u32) -> CompressedFrame {
    use std::time::Instant;
    
    // Données simulées (équivalent d'audio Opus compressé)
    let mut test_data = vec![0u8; 200]; // ~200 bytes typique pour Opus
    
    // Rempli avec un pattern basé sur le numéro de séquence
    for (i, byte) in test_data.iter_mut().enumerate() {
        *byte = ((sequence.wrapping_mul(31) + i as u32) & 0xFF) as u8;
    }
    
    CompressedFrame::new(
        test_data,
        960, // 20ms à 48kHz = 960 échantillons
        Instant::now(),
        sequence as u64,
    )
}
