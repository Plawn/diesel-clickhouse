# Backends diesel-clickhouse

diesel-clickhouse supporte deux backends pour se connecter à ClickHouse :

| Backend | Protocole | Ports | Performance | Cas d'usage |
|---------|-----------|-------|-------------|-------------|
| **HTTP** | HTTP/HTTPS | 8123 / 8443 | Bon | Défaut, facile à déployer |
| **Native** | TCP binaire | 9000 / 9440 | Meilleur | Haute performance |

## HTTP Backend (défaut)

Le backend HTTP utilise l'interface REST de ClickHouse. C'est le choix par défaut car il fonctionne à travers les proxys, load balancers et firewalls.

### Activation

```toml
[dependencies]
diesel-clickhouse = { version = "0.1", features = ["http"] }

# Avec TLS
diesel-clickhouse = { version = "0.1", features = ["http", "rustls-tls"] }
# ou
diesel-clickhouse = { version = "0.1", features = ["http", "native-tls"] }
```

### Connexion

```rust
use diesel_clickhouse::http::ClickHouseConnection;

// Sans TLS (port 8123)
let conn = ClickHouseConnection::new("http://localhost:8123/default").await?;

// Avec TLS (port 8443)
let conn = ClickHouseConnection::new("https://localhost:8443/default").await?;

// Avec authentification
let conn = ClickHouseConnection::new("http://user:password@localhost:8123/mydb").await?;
```

### Requêtes

```rust
use diesel_clickhouse::prelude::*;
use clickhouse::Row;
use serde::Deserialize;

// Le type de résultat doit implémenter clickhouse::Row
#[derive(Row, Deserialize)]
struct User {
    id: u64,
    name: String,
}

// Requête avec le query builder
let users: Vec<User> = conn
    .query(users::table.filter(users::active.eq(true)))
    .fetch_all()
    .await?;

// Requête SQL brute
let users: Vec<User> = conn
    .client()
    .query("SELECT id, name FROM users WHERE active = 1")
    .fetch_all()
    .await?;
```

### Insertions

```rust
use clickhouse::Row;
use serde::Serialize;

#[derive(Row, Serialize)]
struct NewUser {
    id: u64,
    name: String,
}

// Via l'inserter (streaming, efficace pour gros volumes)
let mut inserter = conn.inserter::<NewUser, _>(users::table).await?;
inserter.write(&NewUser { id: 1, name: "Alice".into() }).await?;
inserter.write(&NewUser { id: 2, name: "Bob".into() }).await?;
inserter.end().await?;

// Via SQL brut
conn.insert_raw("users", "(1, 'Alice'), (2, 'Bob')").await?;
```

---

## Native Backend

Le backend natif utilise le protocole binaire TCP de ClickHouse. Il offre de meilleures performances car il évite l'overhead HTTP et utilise un format binaire optimisé.

### Activation

```toml
[dependencies]
diesel-clickhouse = { version = "0.1", features = ["native"] }

# Avec TLS
diesel-clickhouse = { version = "0.1", features = ["native", "native-tls-native"] }
```

### Format de l'URL

```
tcp://[user:password@]host[:port]/database[?options]
```

**Options disponibles :**

| Option | Description | Défaut |
|--------|-------------|--------|
| `secure` | Active TLS | `false` |
| `skip_verify` | Ignore la vérification du certificat | `false` |
| `compression` | Compression LZ4 | `none` |
| `connection_timeout` | Timeout de connexion | `500ms` |
| `query_timeout` | Timeout des requêtes | `180s` |
| `pool_min` | Connexions minimum dans le pool | `1` |
| `pool_max` | Connexions maximum dans le pool | `10` |

### Connexion

```rust
use diesel_clickhouse::native::NativeConnection;

// Sans TLS (port 9000)
let conn = NativeConnection::establish("tcp://localhost:9000/default").await?;

// Avec TLS (port 9440)
let conn = NativeConnection::establish(
    "tcp://localhost:9440/default?secure=true"
).await?;

// Avec authentification et options
let conn = NativeConnection::establish(
    "tcp://admin:secret@clickhouse.example.com:9000/analytics?compression=lz4&pool_max=20"
).await?;
```

### Requêtes

```rust
use diesel_clickhouse::prelude::*;

// Requête avec le query builder
let block = conn.query(
    users::table.filter(users::active.eq(true))
).await?;

// Itérer sur les résultats
for row in block.rows() {
    let id: u64 = row.get("id")?;
    let name: &str = row.get("name")?;
    println!("{}: {}", id, name);
}

// Requête SQL brute
let block = conn.query_raw("SELECT id, name FROM users WHERE active = 1").await?;
```

### Insertions

```rust
use diesel_clickhouse::native::NativeBlock;

// Via Block (efficace pour gros volumes)
let block = NativeBlock::new()
    .column("id", vec![1u64, 2, 3])
    .column("name", vec!["Alice", "Bob", "Charlie"]);

conn.insert("users", block).await?;

// Via SQL brut
conn.insert_values("users", "(1, 'Alice'), (2, 'Bob')").await?;
```

---

## Comparaison détaillée

### Performance

| Opération | HTTP | Native | Différence |
|-----------|------|--------|------------|
| Latence connexion | ~5ms | ~2ms | Native 2x plus rapide |
| Overhead par requête | ~1ms | ~0.1ms | Native 10x moins d'overhead |
| Débit INSERT | Bon | Excellent | Native ~30% plus rapide |
| Débit SELECT | Bon | Excellent | Native ~20% plus rapide |

*Note: Les chiffres varient selon le réseau et la taille des données.*

### Fonctionnalités

| Fonctionnalité | HTTP | Native |
|----------------|------|--------|
| Query builder diesel | ✅ | ✅ |
| TLS | ✅ | ✅ |
| Compression | ✅ (via HTTP) | ✅ (LZ4) |
| Connection pooling | ❌ (manuel) | ✅ (intégré) |
| Streaming results | ✅ | ✅ |
| Progress tracking | ❌ | ✅ |
| Fonctionne derrière proxy | ✅ | ❌ |
| Fonctionne avec CDN | ✅ | ❌ |

### Quand utiliser HTTP

- Déploiement derrière un reverse proxy (nginx, haproxy)
- Accès via un CDN ou load balancer
- Environnement cloud avec restrictions réseau
- Développement local simple
- Intégration avec des outils de monitoring HTTP

### Quand utiliser Native

- Haute performance requise
- Communication directe serveur-à-serveur
- Gros volumes de données
- Besoin de progress tracking
- Connection pooling intégré souhaité

---

## Utiliser les deux backends

Vous pouvez activer les deux backends et choisir à runtime :

```toml
[dependencies]
diesel-clickhouse = { version = "0.1", features = ["http", "native"] }
```

```rust
use diesel_clickhouse::http::ClickHouseConnection;
use diesel_clickhouse::native::NativeConnection;

async fn connect(use_native: bool) -> Result<(), Error> {
    if use_native {
        let conn = NativeConnection::establish("tcp://localhost:9000/default").await?;
        // utiliser conn...
    } else {
        let conn = ClickHouseConnection::new("http://localhost:8123/default").await?;
        // utiliser conn...
    }
    Ok(())
}
```

---

## Configuration TLS

### HTTP avec TLS

```rust
// rustls (recommandé, pur Rust)
// Cargo.toml: features = ["http", "rustls-tls"]
let conn = ClickHouseConnection::new("https://localhost:8443/default").await?;

// native-tls (utilise OpenSSL)
// Cargo.toml: features = ["http", "native-tls"]
let conn = ClickHouseConnection::new("https://localhost:8443/default").await?;
```

### Native avec TLS

```rust
// Cargo.toml: features = ["native", "native-tls-native"]

// TLS standard
let conn = NativeConnection::establish(
    "tcp://localhost:9440/default?secure=true"
).await?;

// TLS sans vérification (dev uniquement!)
let conn = NativeConnection::establish(
    "tcp://localhost:9440/default?secure=true&skip_verify=true"
).await?;
```

---

## Ports ClickHouse

| Service | Port défaut | Description |
|---------|-------------|-------------|
| HTTP | 8123 | Interface HTTP sans TLS |
| HTTPS | 8443 | Interface HTTP avec TLS |
| Native | 9000 | Protocole natif sans TLS |
| Native TLS | 9440 | Protocole natif avec TLS |

Vérifiez votre configuration ClickHouse (`config.xml`) pour les ports exacts.
