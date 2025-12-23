# Template d'Audit Performance - diesel-clickhouse

> **Instructions pour Claude Code**: Ce document est un template pour auditer les performances de la codebase diesel-clickhouse. Suivez chaque section dans l'ordre, exécutez les commandes indiquées avec l'outil `Bash`, lisez les fichiers avec `Read`, et remplissez les résultats dans les tableaux. Marquez les tâches complétées avec `[x]`.

---

## Informations de l'Audit

| Champ | Valeur |
|-------|--------|
| **Date** | _À remplir_ |
| **Commit/Version** | _Exécuter: `git rev-parse --short HEAD`_ |
| **Auditeur** | Claude Code |
| **Durée estimée** | 30-60 minutes |

---

## 1. Setup et Vérification Environnement

### 1.1 Commandes de Vérification

**Action Claude Code**: Exécuter ces commandes pour valider l'environnement.

```bash
# Version Rust
rustc --version

# Vérifier compilation
cargo check --all-features

# Compter les lignes de code
cloc diesel-clickhouse*/src/ --by-file --include-lang=Rust 2>/dev/null || find diesel-clickhouse*/src -name "*.rs" -exec wc -l {} + | tail -1
```

### 1.2 Résultats Environnement

| Vérification | Résultat | Status |
|--------------|----------|--------|
| Version Rust | | |
| Compilation | | |
| Lignes de code total | | |

---

## 2. Architecture et Fichiers Critiques

### 2.1 Structure des Crates

**Action Claude Code**: Vérifier la structure avec `ls` et analyser les tailles.

```bash
# Lister les crates
ls -la diesel-clickhouse*/

# Taille par crate
du -sh diesel-clickhouse*/src/
```

### 2.2 Fichiers Performance-Critiques à Analyser

| Fichier | Priorité | Analysé | Notes |
|---------|----------|---------|-------|
| `diesel-clickhouse/src/native.rs` | CRITIQUE | [ ] | Backend Native, streaming |
| `diesel-clickhouse/src/stream.rs` | CRITIQUE | [ ] | Implémentation streaming |
| `diesel-clickhouse/src/http.rs` | HAUTE | [ ] | Backend HTTP |
| `diesel-clickhouse-core/src/query_builder/arena.rs` | HAUTE | [ ] | Arena allocation |
| `diesel-clickhouse-core/src/interner.rs` | HAUTE | [ ] | String interning |
| `diesel-clickhouse/src/batch.rs` | MOYENNE | [ ] | Batch insertion |
| `diesel-clickhouse/src/zero_copy.rs` | MOYENNE | [ ] | Zero-copy parsing |
| `diesel-clickhouse/src/pool.rs` | MOYENNE | [ ] | Connection pooling |
| `diesel-clickhouse/src/serialize.rs` | MOYENNE | [ ] | Sérialisation |
| `diesel-clickhouse/src/async_insert.rs` | BASSE | [ ] | Async insert |

---

## 3. Audit du Native Backend (PRIORITÉ CRITIQUE)

### 3.1 Analyse du Streaming

**Action Claude Code**:
1. Lire `diesel-clickhouse/src/native.rs`
2. Chercher les patterns qui chargent tout en mémoire

```bash
# Chercher fetch_all, collect, to_vec
grep -n "fetch_all\|\.collect()\|\.to_vec()\|into_iter().collect" diesel-clickhouse/src/native.rs

# Chercher dans stream.rs
grep -n "fetch_all\|Vec<.*>\s*=" diesel-clickhouse/src/stream.rs
```

### 3.2 Questions à Répondre

| Question | Réponse | Sévérité |
|----------|---------|----------|
| Le native backend charge-t-il tout en mémoire via `fetch_all()`? | | 🔴/🟡/🟢 |
| Y a-t-il un vrai streaming row-by-row? | | 🔴/🟡/🟢 |
| Quelle est la struct utilisée pour itérer les résultats? | | Info |
| Le HTTP backend a-t-il le même problème? | | 🔴/🟡/🟢 |

### 3.3 Code Suspect Identifié

| Fichier:Ligne | Code | Problème |
|---------------|------|----------|
| | | |

---

## 4. Audit du Query Builder

### 4.1 Arena Allocation

**Action Claude Code**: Lire `diesel-clickhouse-core/src/query_builder/arena.rs`

```bash
# Vérifier configuration arena
grep -n "with_capacity\|ARENA\|thread_local\|Bump" diesel-clickhouse-core/src/query_builder/arena.rs

# Vérifier réutilisation
grep -n "reset\|clear\|drop" diesel-clickhouse-core/src/query_builder/arena.rs
```

### 4.2 Questions Arena

| Question | Réponse | Note |
|----------|---------|------|
| Capacité initiale de l'arena (bytes)? | | Chercher `with_capacity` |
| Thread-local ou global? | | Chercher `thread_local!` |
| Reset automatique après query? | | Chercher `reset()` |
| Pre-allocation des parts? | | Chercher `SmallVec` capacity |

### 4.3 String Interning

**Action Claude Code**: Lire `diesel-clickhouse-core/src/interner.rs`

```bash
# Vérifier type de lock
grep -n "RwLock\|Mutex\|parking_lot" diesel-clickhouse-core/src/interner.rs

# Vérifier fast path
grep -n "read()\|write()\|get_or_intern" diesel-clickhouse-core/src/interner.rs
```

### 4.4 Questions Interning

| Question | Réponse | Note |
|----------|---------|------|
| Type de lock utilisé? | | RwLock = bon, Mutex = à vérifier |
| Fast path avec read lock? | | Pattern two-level locking |
| Capacité initiale? | | Chercher `with_capacity` |

---

## 5. Audit des Allocations

### 5.1 Patterns d'Allocation à Chercher

**Action Claude Code**: Exécuter ces recherches sur l'ensemble du code.

```bash
# Vec::new() sans capacité (potentiel problème)
grep -rn "Vec::new()" diesel-clickhouse*/src/ --include="*.rs" | grep -v test | head -20

# String allocations dans hot paths
grep -rn "\.to_string()\|\.to_owned()\|String::from" diesel-clickhouse*/src/ --include="*.rs" | grep -v test | wc -l

# Clone potentiellement coûteux
grep -rn "\.clone()" diesel-clickhouse*/src/ --include="*.rs" | grep -v test | wc -l

# format! dans code non-debug
grep -rn "format!" diesel-clickhouse*/src/ --include="*.rs" | grep -v test | grep -v "Debug\|Display" | head -20
```

### 5.2 Résultats Allocations

| Pattern | Occurrences | Fichiers Principaux | Sévérité |
|---------|-------------|---------------------|----------|
| `Vec::new()` sans capacité | | | 🔴/🟡/🟢 |
| `.to_string()` | | | 🔴/🟡/🟢 |
| `.clone()` | | | 🔴/🟡/🟢 |
| `format!` hors debug | | | 🔴/🟡/🟢 |

### 5.3 Vérification SmallVec/Optimisations

```bash
# SmallVec usage
grep -rn "SmallVec\|smallvec" diesel-clickhouse*/src/ --include="*.rs" | wc -l

# compact_str usage
grep -rn "CompactString\|compact_str" diesel-clickhouse*/src/ --include="*.rs" | wc -l

# itoa/ryu usage (bonne pratique)
grep -rn "itoa\|ryu" diesel-clickhouse*/src/ --include="*.rs" | wc -l
```

### 5.4 Optimisations Présentes

| Optimisation | Présente | Fichiers |
|--------------|----------|----------|
| SmallVec pour petits vecs | [ ] Oui / [ ] Non | |
| compact_str pour strings | [ ] Oui / [ ] Non | |
| itoa pour int→string | [ ] Oui / [ ] Non | |
| ryu pour float→string | [ ] Oui / [ ] Non | |
| Arena allocation | [ ] Oui / [ ] Non | |

---

## 6. Audit Async/Concurrence

### 6.1 Locks dans Code Async

**Action Claude Code**: Identifier les locks potentiellement bloquants.

```bash
# std::sync::Mutex dans code async (potentiel problème)
grep -rn "std::sync::Mutex\|sync::Mutex" diesel-clickhouse*/src/ --include="*.rs" | grep -v test

# tokio::sync::Mutex (correct)
grep -rn "tokio::sync::Mutex" diesel-clickhouse*/src/ --include="*.rs" | grep -v test

# RwLock usage
grep -rn "RwLock" diesel-clickhouse*/src/ --include="*.rs" | grep -v test
```

### 6.2 Questions Concurrence

| Question | Réponse | Sévérité |
|----------|---------|----------|
| `std::sync::Mutex` dans contexte async? | | 🔴/🟡/🟢 |
| Sections critiques courtes? | | 🔴/🟡/🟢 |
| Deadlock potentiel identifié? | | 🔴/🟡/🟢 |

### 6.3 Box<dyn Future> (Anti-pattern)

```bash
# Chercher Box<dyn Future> (allocation heap pour futures)
grep -rn "Box<dyn Future\|Box<dyn.*Future" diesel-clickhouse*/src/ --include="*.rs" | grep -v test
```

| Fichier:Ligne | Justifié? | Note |
|---------------|-----------|------|
| | | |

---

## 7. Audit Sérialisation

### 7.1 Formats et Performance

**Action Claude Code**: Analyser les formats de sérialisation.

```bash
# Formats supportés
grep -rn "RowBinary\|TabSeparated\|JSON\|Arrow" diesel-clickhouse*/src/ --include="*.rs" | head -20

# Format par défaut
grep -rn "default.*format\|FORMAT" diesel-clickhouse*/src/ --include="*.rs" | head -10
```

### 7.2 Numérique vers String

```bash
# Vérifier si itoa est utilisé pour les entiers
grep -rn "itoa::Buffer\|itoa::write" diesel-clickhouse*/src/ --include="*.rs"

# Vérifier si ryu est utilisé pour les floats
grep -rn "ryu::Buffer\|ryu::write" diesel-clickhouse*/src/ --include="*.rs"

# format! sur nombres (anti-pattern)
grep -rn 'format!.*\{:.*\}' diesel-clickhouse*/src/serialize.rs diesel-clickhouse-core/src/serialize.rs 2>/dev/null
```

### 7.3 Questions Sérialisation

| Question | Réponse | Note |
|----------|---------|------|
| Format par défaut? | | RowBinary = optimal |
| itoa utilisé? | | 2-5x vs format! |
| ryu utilisé? | | 2-10x vs format! |
| Zero-copy disponible? | | TSV ou Arrow |

---

## 8. Audit Connection Pool

### 8.1 Configuration Pool

**Action Claude Code**: Lire `diesel-clickhouse/src/pool.rs`

```bash
# Chercher les valeurs par défaut
grep -n "default\|DEFAULT\|max_size\|min_idle\|timeout" diesel-clickhouse/src/pool.rs | head -20

# Librairie utilisée
grep -n "deadpool\|bb8\|mobc\|r2d2" diesel-clickhouse/Cargo.toml
```

### 8.2 Questions Pool

| Question | Réponse | Note |
|----------|---------|------|
| Librairie de pooling? | | |
| Max connections par défaut? | | |
| Min idle? | | |
| Connection timeout? | | |
| Idle timeout? | | |
| Warm-up parallélisé? | | Chercher `join_all` |

---

## 9. Audit Batch Insert

### 9.1 Analyse Batch

**Action Claude Code**: Lire `diesel-clickhouse/src/batch.rs`

```bash
# Pre-allocation buffer
grep -n "with_capacity\|capacity\|batch_size" diesel-clickhouse/src/batch.rs

# SQL string building
grep -n "String::new\|push_str\|format!" diesel-clickhouse/src/batch.rs
```

### 9.2 Questions Batch

| Question | Réponse | Note |
|----------|---------|------|
| Buffer pré-alloué avec capacité? | | |
| Batch size par défaut? | | |
| SQL string pré-estimée? | | |
| Clone des données dans boucle? | | Chercher `.clone()` |

---

## 10. Analyse Clippy

### 10.1 Exécution Clippy

**Action Claude Code**: Exécuter clippy avec les lints performance.

```bash
# Clippy standard
cargo clippy --all-features 2>&1 | grep -E "warning|error" | head -30

# Clippy performance spécifique
cargo clippy --all-features -- \
  -W clippy::inefficient_to_string \
  -W clippy::large_enum_variant \
  -W clippy::needless_collect \
  -W clippy::trivially_copy_pass_by_ref \
  -W clippy::vec_init_then_push \
  2>&1 | grep -E "warning|error" | head -30
```

### 10.2 Résultats Clippy

| Warning | Fichier | Ligne | Action Requise |
|---------|---------|-------|----------------|
| | | | |

---

## 11. Audit Code Unsafe

### 11.1 Recenser Unsafe

```bash
# Compter les blocs unsafe
grep -rn "unsafe" diesel-clickhouse*/src/ --include="*.rs" | grep -v test | wc -l

# Lister les blocs unsafe
grep -rn "unsafe {" diesel-clickhouse*/src/ --include="*.rs" | grep -v test
```

### 11.2 Review Unsafe

| Fichier:Ligne | Justification | Risque | Review |
|---------------|---------------|--------|--------|
| | | | [ ] OK / [ ] À revoir |

---

## 12. Tests de Compilation

### 12.1 Temps de Build

**Action Claude Code**: Mesurer les temps.

```bash
# Clean build (si le temps le permet)
cargo clean && time cargo build --all-features 2>&1 | tail -5

# Ou juste check
time cargo check --all-features 2>&1 | tail -3
```

### 12.2 Taille Binaire

```bash
# Build release CLI
cargo build --release -p diesel-clickhouse-cli 2>/dev/null

# Taille
ls -lh target/release/diesel-clickhouse-cli 2>/dev/null || echo "Build CLI first"
```

### 12.3 Métriques Build

| Métrique | Valeur | Acceptable |
|----------|--------|------------|
| Temps check | | < 30s |
| Temps build release | | < 3min |
| Taille binaire CLI | | < 20MB |

---

## 13. Dépendances

### 13.1 Analyse Dépendances

```bash
# Arbre des dépendances (résumé)
cargo tree -p diesel-clickhouse --features "http,native" -e normal --depth 1

# Dépendances dupliquées
cargo tree -p diesel-clickhouse -d 2>/dev/null | head -20
```

### 13.2 Dépendances Performance

| Dépendance | Usage | Version | OK |
|------------|-------|---------|-----|
| tokio | Runtime async | | [ ] |
| bumpalo | Arena alloc | | [ ] |
| smallvec | Stack vecs | | [ ] |
| itoa | Int formatting | | [ ] |
| ryu | Float formatting | | [ ] |
| clickhouse | HTTP backend | | [ ] |
| clickhouse-rs | Native backend | | [ ] |

---

## 14. Résumé des Findings

### 14.1 Problèmes Critiques (Bloquants)

| ID | Description | Fichier:Ligne | Impact | Recommandation |
|----|-------------|---------------|--------|----------------|
| C1 | | | | |
| C2 | | | | |

### 14.2 Problèmes Majeurs

| ID | Description | Fichier:Ligne | Impact | Recommandation |
|----|-------------|---------------|--------|----------------|
| M1 | | | | |
| M2 | | | | |

### 14.3 Problèmes Mineurs

| ID | Description | Fichier:Ligne | Impact | Recommandation |
|----|-------------|---------------|--------|----------------|
| m1 | | | | |
| m2 | | | | |

### 14.4 Points Positifs

| Aspect | Description | Fichier |
|--------|-------------|---------|
| | | |

---

## 15. Score et Recommandations

### 15.1 Score par Catégorie

| Catégorie | Score /10 | Notes |
|-----------|-----------|-------|
| Architecture | | |
| Gestion mémoire | | |
| Sérialisation | | |
| Async/Streaming | | |
| Query building | | |
| **Score Global** | **/10** | |

### 15.2 Plan d'Action

#### Haute Priorité (Quick Wins)

| # | Action | Fichier | Effort | Impact |
|---|--------|---------|--------|--------|
| 1 | | | | |

#### Moyenne Priorité

| # | Action | Fichier | Effort | Impact |
|---|--------|---------|--------|--------|
| 1 | | | | |

#### Basse Priorité

| # | Action | Fichier | Effort | Impact |
|---|--------|---------|--------|--------|
| 1 | | | | |

---

## 16. Checklist Finale

- [ ] Section 1: Environnement vérifié
- [ ] Section 2: Fichiers critiques identifiés
- [ ] Section 3: Native backend analysé
- [ ] Section 4: Query builder analysé
- [ ] Section 5: Allocations auditées
- [ ] Section 6: Async/Concurrence vérifiée
- [ ] Section 7: Sérialisation analysée
- [ ] Section 8: Pool analysé
- [ ] Section 9: Batch insert analysé
- [ ] Section 10: Clippy exécuté
- [ ] Section 11: Unsafe reviewé
- [ ] Section 12: Métriques build collectées
- [ ] Section 13: Dépendances vérifiées
- [ ] Section 14: Findings documentés
- [ ] Section 15: Score calculé et recommandations

---

## Annexe A: Commandes de Référence

```bash
# Profiling mémoire (nécessite dhat)
DHAT_LOG=dhat-heap.json cargo test --release

# Flamegraph CPU (nécessite cargo-flamegraph)
cargo flamegraph --bin diesel-clickhouse-cli -- migrate

# Comptage allocations avec DHAT
cargo +nightly test --release -- --nocapture

# Analyse taille types
cargo +nightly rustc -- -Z print-type-sizes 2>&1 | head -50

# Benchmark timing compilation
cargo build --timings --release
```

## Annexe B: Seuils de Performance Recommandés

| Métrique | Bon | Acceptable | Mauvais |
|----------|-----|------------|---------|
| Allocations par query | < 10 | < 50 | > 100 |
| Temps build check | < 15s | < 30s | > 60s |
| Clone dans hot path | 0 | < 5 | > 10 |
| Box<dyn Future> | 0-1 | 2-3 | > 5 |
| unsafe blocs | < 5 | < 10 | > 20 |

## Annexe C: Patterns à Éviter

```rust
// MAUVAIS: allocation dans boucle
for item in items {
    result.push(item.to_string());  // Allocation!
}

// BON: pré-allocation
let mut result = Vec::with_capacity(items.len());
for item in items {
    result.push(item);
}

// MAUVAIS: format! pour nombres
let s = format!("{}", number);

// BON: itoa/ryu
let mut buf = itoa::Buffer::new();
let s = buf.format(number);

// MAUVAIS: clone HashMap keys
map.insert(key.clone(), value);

// BON: Arc pour partage
map.insert(Arc::clone(&key), value);
```

---

**Template créé**: 2025-12-23
**Version**: 1.0
**Pour**: diesel-clickhouse
