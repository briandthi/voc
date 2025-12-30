// Application de test pour le système audio Voc
// 
// Cette application permet de tester tous les composants audio :
// - Test des périphériques audio
// - Test du codec Opus
// - Test du pipeline complet
// - Mesures de performance et latence

use std::io::{self, Write};

use audio::{
    AudioConfig, AudioPipelineImpl, AudioPipeline,
    CpalCapture, CpalPlayback, OpusCodec,
    AudioCapture, AudioPlayback, AudioCodec,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎤 Application de test audio Voc");
    println!("==================================");
    
    // Test de la configuration
    println!("\n1️⃣  Test de la configuration...");
    test_config()?;
    
    // Test des périphériques
    println!("\n2️⃣  Test des périphériques audio...");
    test_devices().await?;
    
    // Test du codec Opus
    println!("\n3️⃣  Test du codec Opus...");
    test_codec()?;
    
    // Menu interactif
    loop {
        println!("\n🎛️  Menu principal :");
        println!("   1 - Test loopback (micro → haut-parleurs)");
        println!("   2 - Test de performance");
        println!("   3 - Test de stress");
        println!("   4 - Informations système");
        println!("   q - Quitter");
        
        print!("Votre choix : ");
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        
        match input.trim() {
            "1" => test_loopback().await?,
            "2" => test_performance().await?,
            "3" => test_stress().await?,
            "4" => show_system_info().await?,
            "q" | "Q" => break,
            _ => println!("❌ Choix invalide"),
        }
    }
    
    println!("👋 Au revoir !");
    Ok(())
}

/// Test de la configuration audio
fn test_config() -> Result<(), Box<dyn std::error::Error>> {
    let config = AudioConfig::default();
    
    // Valide la configuration
    config.validate()?;
    
    println!("✅ Configuration validée :");
    println!("   Sample rate : {} Hz", config.sample_rate);
    println!("   Channels : {}", config.channels);
    println!("   Frame duration : {}ms", config.frame_duration_ms);
    println!("   Opus bitrate : {} bps", config.opus_bitrate);
    println!("   Échantillons par frame : {}", config.samples_per_frame());
    println!("   Latence théorique : {}ms", config.theoretical_latency_ms());
    
    Ok(())
}

/// Test des périphériques audio
async fn test_devices() -> Result<(), Box<dyn std::error::Error>> {
    let config = AudioConfig::default();
    
    // Test du microphone
    print!("🎤 Test du microphone... ");
    match CpalCapture::new(config.clone()) {
        Ok(capture) => {
            println!("✅ {}", capture.device_info());
        },
        Err(e) => {
            println!("❌ Erreur : {}", e);
            return Err(e.into());
        }
    }
    
    // Test des haut-parleurs
    print!("🔊 Test des haut-parleurs... ");
    match CpalPlayback::new(config) {
        Ok(playback) => {
            println!("✅ {}", playback.device_info());
        },
        Err(e) => {
            println!("❌ Erreur : {}", e);
            return Err(e.into());
        }
    }
    
    Ok(())
}

/// Test du codec Opus
fn test_codec() -> Result<(), Box<dyn std::error::Error>> {
    let config = AudioConfig::default();
    let mut codec = OpusCodec::new(config.clone())?;
    
    println!("🎵 Codec : {}", codec.codec_info());
    
    // Test avec différents types de signaux
    test_codec_with_signal(&mut codec, "silence", create_silence(&config))?;
    test_codec_with_signal(&mut codec, "bruit blanc", create_white_noise(&config))?;
    test_codec_with_signal(&mut codec, "onde sinusoïdale", create_sine_wave(&config, 440.0))?;
    
    println!("✅ Tous les tests codec réussis");
    Ok(())
}

/// Test du codec avec un signal spécifique
fn test_codec_with_signal(
    codec: &mut OpusCodec, 
    signal_name: &str, 
    samples: Vec<f32>
) -> Result<(), Box<dyn std::error::Error>> {
    use audio::AudioFrame;
    
    let frame = AudioFrame::new(samples, 0);
    
    // Test encodage
    let compressed = codec.encode(&frame)?;
    
    // Test décodage
    let decoded = codec.decode(&compressed)?;
    
    // Calcule l'erreur RMS
    let mut error_sum = 0.0;
    for (orig, decoded) in frame.samples.iter().zip(decoded.samples.iter()) {
        let error = orig - decoded;
        error_sum += error * error;
    }
    let rms_error = (error_sum / frame.samples.len() as f32).sqrt();
    
    println!("   {} : {:.1}x compression, erreur RMS: {:.4}", 
             signal_name, 
             compressed.compression_ratio(), 
             rms_error);
    
    Ok(())
}

/// Crée un signal de silence
fn create_silence(config: &AudioConfig) -> Vec<f32> {
    vec![0.0; config.samples_per_frame()]
}

/// Crée un bruit blanc
fn create_white_noise(config: &AudioConfig) -> Vec<f32> {
    use rand::prelude::*;
    let mut rng = thread_rng();
    (0..config.samples_per_frame())
        .map(|_| rng.gen_range(-0.1..0.1))
        .collect()
}

/// Crée une onde sinusoïdale
fn create_sine_wave(config: &AudioConfig, frequency: f32) -> Vec<f32> {
    let sample_rate = config.sample_rate as f32;
    (0..config.samples_per_frame())
        .map(|i| {
            let t = i as f32 / sample_rate;
            (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5
        })
        .collect()
}

/// Test loopback interactif
async fn test_loopback() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔄 Test Loopback");
    println!("================");
    println!("⚠️  Attention : Vous allez entendre votre propre voix !");
    println!("⚠️  Éloignez le microphone des haut-parleurs pour éviter le larsen.");
    
    print!("Durée du test (secondes, 1-30) : ");
    io::stdout().flush().unwrap();
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    let duration: u32 = input.trim().parse().unwrap_or(5).clamp(1, 30);
    
    println!("\n🚀 Démarrage du test loopback pour {}s...", duration);
    println!("💬 Parlez dans le microphone !");
    
    let config = AudioConfig::default();
    let mut pipeline = AudioPipelineImpl::new(config)?;
    
    match pipeline.run_loopback_test(duration).await {
        Ok(stats) => {
            println!("\n📊 Résultats du test :");
            println!("   ✅ Test terminé avec succès");
            println!("   📈 Frames traitées : {}", stats.frames_captured);
            println!("   🕐 Latence moyenne : {:.1}ms", stats.avg_latency_ms);
            println!("   🔊 Niveau audio : {:.3}", stats.avg_rms_level);
            println!("   📦 Compression : {:.1}x", stats.avg_compression_ratio);
            
            if stats.buffer_overflows > 0 {
                println!("   ⚠️  Overflows : {}", stats.buffer_overflows);
            }
        },
        Err(e) => {
            println!("❌ Erreur pendant le test : {}", e);
        }
    }
    
    Ok(())
}

/// Test de performance
async fn test_performance() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⚡ Test de Performance");
    println!("=====================");
    
    let config = AudioConfig::default();
    let mut pipeline = AudioPipelineImpl::new(config)?;
    
    println!("🔬 Test de performance (10 secondes)...");
    
    match pipeline.performance_test(10).await {
        Ok(_) => {
            println!("✅ Test de performance terminé");
        },
        Err(e) => {
            println!("❌ Erreur : {}", e);
        }
    }
    
    Ok(())
}

/// Test de stress
async fn test_stress() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n💪 Test de Stress");
    println!("=================");
    
    let config = AudioConfig::default();
    let mut pipeline = AudioPipelineImpl::new(config)?;
    
    println!("🏋️  Test de stress (15 secondes)...");
    println!("📊 Simulation de charge CPU élevée...");
    
    match pipeline.stress_test(15).await {
        Ok(_) => {
            println!("✅ Test de stress terminé");
        },
        Err(e) => {
            println!("❌ Erreur : {}", e);
        }
    }
    
    Ok(())
}

/// Affiche les informations système
async fn show_system_info() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n💻 Informations Système");
    println!("=======================");
    
    let config = AudioConfig::default();
    
    println!("🔧 Configuration :");
    println!("   Sample rate : {} Hz", config.sample_rate);
    println!("   Échantillons par frame : {}", config.samples_per_frame());
    println!("   Taille frame brute : {} bytes", config.frame_size_bytes());
    println!("   Latence théorique : {}ms", config.theoretical_latency_ms());
    
    println!("\n🎤 Périphériques :");
    if let Ok(capture) = CpalCapture::new(config.clone()) {
        println!("   Entrée : {}", capture.device_info());
    }
    if let Ok(playback) = CpalPlayback::new(config) {
        println!("   Sortie : {}", playback.device_info());
    }
    
    println!("\n💾 Mémoire :");
    println!("   Taille AudioFrame : {} bytes", std::mem::size_of::<audio::AudioFrame>());
    println!("   Taille CompressedFrame : {} bytes", std::mem::size_of::<audio::CompressedFrame>());
    
    println!("\n🚀 Performance :");
    println!("   Threads disponibles : {}", num_cpus::get());
    
    Ok(())
}
