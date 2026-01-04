# Guide de Test Voc - 2 PCs Windows sur Réseau Local

## Vue d'ensemble

Maintenant que la partie 3 (networking UDP) est terminée, tu peux tester la communication P2P entre deux PCs Windows sur le même réseau local. Voici le guide complet.

## Prérequis sur Chaque PC Windows

### 1. Installation de Rust
```powershell
# Télécharge et installe Rust depuis https://rustup.rs/
# Ou utilise winget si disponible :
winget install Rustlang.Rust.GNU
```

### 2. Dépendances Windows Spécifiques
```powershell
# Installe Visual Studio Build Tools (requis pour compiler certaines dépendances)
# Télécharge depuis : https://visualstudio.microsoft.com/visual-cpp-build-tools/

# Ou installe Visual Studio Community avec workload "C++ build tools"
```

### 3. Configuration du Firewall Windows

**⚠️ ÉTAPE CRITIQUE** - Le firewall Windows bloque par défaut les connexions UDP entrantes.

#### Option A : Désactiver temporairement (pour tests uniquement)
```powershell
# Exécute PowerShell en tant qu'administrateur
# Désactive temporairement le firewall pour le profil privé
netsh advfirewall set privateprofile state off
```

#### Option B : Créer une règle spécifique (recommandé)
```powershell
# Exécute PowerShell en tant qu'administrateur
# Autorise le trafic UDP sur le port 9001
netsh advfirewall firewall add rule name="Voc Audio App" dir=in action=allow protocol=UDP localport=9001
```

## Compilation du Projet

### Sur chaque PC, clone et compile :
```powershell
# Clone le projet
git clone <ton-repo-voc>
cd voc

# Compile en mode release pour de meilleures performances
cargo build --release --bin voc-client

# Vérifie que la compilation réussit
.\target\release\voc-client.exe --help
```

## Configuration Réseau

### 1. Identifie les Adresses IP
Sur chaque PC, trouve l'adresse IP locale :
```powershell
ipconfig | findstr "IPv4"
```

Exemple de résultat :
```
PC1: 192.168.1.100
PC2: 192.168.1.150
```

### 2. Test de Connectivité de Base
Teste la connectivité réseau entre les PCs :
```powershell
# Depuis PC1, ping PC2
ping 192.168.1.150

# Depuis PC2, ping PC1  
ping 192.168.1.100
```

## Procédure de Test P2P

### Scénario : PC1 serveur, PC2 client

#### Sur PC1 (Serveur)
```powershell
# Lance le serveur sur le port par défaut (9001)
.\target\release\voc-client.exe listen --port 9001 --verbose

# Tu devrais voir :
# 🚀 Démarrage serveur Voc sur port 9001...
# ✅ Serveur prêt !
# 📡 Connexion possible via :
#    🌍 192.168.1.100:9001
#    🏠 127.0.0.1:9001
```

#### Sur PC2 (Client)
```powershell
# Connecte-toi au serveur PC1 (remplace par l'IP réelle)
.\target\release\voc-client.exe connect --server 192.168.1.100:9001 --verbose --frames 20

# Tu devrais voir :
# 🚀 Client Voc
# 📡 Connexion au serveur 192.168.1.100:9001...
# ✅ Connexion établie avec succès !
# 📤 Envoi de 20 frames de test...
```

### Scénario Inverse : PC2 serveur, PC1 client

Répète la procédure en inversant les rôles pour tester la bidirectionnalité.

## Résultats Attendus

### ✅ Connexion Réussie
```
📈 Résultats :
   ✅ Frames envoyées : 20
   📊 Taux de succès : 100.0%
✅ Test terminé avec succès
```

### ❌ Problèmes Possibles

#### 1. Échec de Connexion
```
❌ Échec de connexion : Connection timed out
```

**Solutions :**
- Vérifie le firewall (règles UDP port 9001)
- Vérifie que les PCs sont sur le même réseau
- Teste avec `telnet <IP> 9001` ou `Test-NetConnection`

#### 2. Perte de Paquets
```
📈 Résultats :
   ✅ Frames envoyées : 15
   ❌ Échecs : 5
   📊 Taux de succès : 75.0%
```

**Causes possibles :**
- Congestion réseau WiFi
- Firewall trop strict
- QoS réseau limitant UDP

#### 3. Erreurs de Compilation
```
error: linker `link.exe` not found
```

**Solution :** Installe Visual Studio Build Tools ou utilise la toolchain GNU :
```powershell
rustup default stable-x86_64-pc-windows-gnu
```

## Tests Avancés

### 1. Test de Performance Réseau
```powershell
# Test avec plus de frames pour mesurer la stabilité
.\target\release\voc-client.exe connect --server 192.168.1.100:9001 --frames 200

# Analyse les statistiques de succès/échec
```

### 2. Test de Reconnexion
```powershell
# Lance plusieurs connexions successives
for ($i=1; $i -le 5; $i++) {
    echo "=== Test $i ==="
    .\target\release\voc-client.exe connect --server 192.168.1.100:9001 --frames 10
    Start-Sleep -Seconds 2
}
```

### 3. Test de Charge (Optionnel)
```powershell
# Lance plusieurs clients en parallèle (attention : le serveur actuel ne gère qu'une connexion)
Start-Job { .\target\release\voc-client.exe connect --server 192.168.1.100:9001 --frames 50 }
```

## Diagnostics Réseau

### Vérification des Ports
```powershell
# Vérifie que le serveur écoute bien
netstat -an | findstr ":9001"

# Doit afficher quelque chose comme :
# UDP    0.0.0.0:9001           *:*
```

### Test avec Outils Réseau
```powershell
# Test de connectivité UDP (avec nc si disponible)
# Ou utilise PowerShell :
Test-NetConnection -ComputerName 192.168.1.100 -Port 9001
```

## Configuration Réseau Optimale

### Pour un Réseau WiFi
- Assure-toi que les deux PCs sont connectés au même réseau WiFi
- Évite les réseaux WiFi publics qui isolent les clients
- Privilégie la bande 5GHz pour moins de congestion

### Pour un Réseau Ethernet
- Connexion directe via switch/hub pour latence minimale
- Configuration auto-négociation des cartes réseau

## Prochaines Étapes

Une fois les tests P2P réussis, tu pourras :

1. **Intégrer l'audio réel** : Remplacer les frames de test par de vraies données audio depuis le microphone
2. **Optimiser les performances** : Ajuster les buffers et timeouts selon tes mesures
3. **Interface utilisateur** : Passer à la phase 4 (Bridge Tauri) pour créer une UI conviviale

## Résumé des Commandes

```powershell
# Compilation
cargo build --release --bin voc-client

# PC Serveur
.\target\release\voc-client.exe listen --port 9001 --verbose

# PC Client (remplace l'IP)
.\target\release\voc-client.exe connect --server <IP_SERVEUR>:9001 --verbose

# Firewall (en admin)
netsh advfirewall firewall add rule name="Voc Audio App" dir=in action=allow protocol=UDP localport=9001
```

C'est un bon test pour valider ton architecture réseau avant d'intégrer l'audio temps réel !
