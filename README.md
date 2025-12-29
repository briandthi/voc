# Projet Voc (voice chat)

## Description
Application de communication vocale peer-to-peer pour réseau local, avec un client Rust performant pour le traitement audio temps réel et une interface utilisateur moderne en TypeScript. L'objectif est de créer une alternative légère à Discord, optimisée pour deux utilisateurs en LAN avec une latence minimale et une qualité audio professionnelle.

## MVP Scope
- Communication audio mono entre 2 pairs sur le LAN
- Connexion manuelle via IP:PORT
- Latence < 50ms en conditions LAN
- Pas de serveur, pas de compte, pas de chiffrement
- Mono-canal uniquement 

## Stack Technique

**Backend (Rust)**
- `tokio` - Runtime asynchrone
- `opus` - Codec audio compression
- `cpal` - Capture et lecture audio
- `serde` - Sérialisation des données

## Networking
- `MVP`: UDP brut avec numéro de séquence pour détecter les pertes de paquets, avec port 9001 par défault (low latency, no retransmission)
- `Future`: QUIC (quinn) pour NAT traversal / WAN

## Audio
- Codec: Opus
- Sample rate: 48kHz
- Channels: Mono
- Frame size: 20ms
- Bitrate cible: 32–64 kbps
- Buffer: ~40-60ms (2-3 frames) pour gérer le jitter

## UI Strategy

### Phase 1: UI minimale (connect / mute / volume)

### Phase 2: UI React complète (visual feedback, settings)

**Frontend (TypeScript)**
- React + Vite
- Tauri - Bridge Rust/TypeScript pour application desktop
- shadcn/ui + Tailwind - Interface utilisateur
- Zustand ou TanStack Query - State management

## Plan du Projet

### 1. Setup de l'environnement

### 2. Core audio Rust
Implémentation de la capture microphone, compression Opus et lecture audio

#### Tests audio isolés
Validation capture → compression → décompression → lecture en local avant networking

### 3. Networking UDP
Création du système d'envoi/réception de paquets audio en peer-to-peer

### 4. Bridge Tauri
Exposition des commandes Rust vers TypeScript (connect, disconnect, mute, volume)

### 5. Interface utilisateur
Design de l'UI Phase 1

### Phase ultérieure
- Découverte automatique LAN (mDNS / UDP broadcast)
- Reconnexion automatique
- Indicateurs
- Liste des pairs disponibles
- UI phase 2
- Quinn
