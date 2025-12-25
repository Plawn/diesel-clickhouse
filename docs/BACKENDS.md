# Backends diesel-clickhouse

diesel-clickhouse supporte deux backends pour se connecter à ClickHouse via une API unifiée :

| Backend | Protocole | Ports | Performance | Cas d'usage |
|---------|-----------|-------|-------------|-------------|
| **HTTP** | HTTP/HTTPS | 8123 / 8443 | Bon | Défaut, facile à déployer |
| **Native** | TCP binaire | 9000 / 9440 | Meilleur | Haute performance |

## API Unifiée (Recommandée)

L'API unifiée `Connection` fonctionne avec les deux backends :

```rust
use diesel_clickhouse::prelude::*;
use diesel_clickhouse::Connection;

// HTTP backend
let http_conn = Connection::http()
    .host("localhost")
    .port(8123)
    .user("default")
    .password("default")
    .database("mydb")
    .build()
    .await?;

// Native backend
let native_conn = Connection::native()
    .host("localhost")
    .port(9000)
    .user("default")
    .password("default")
    .database("mydb")
    .build()
    .await?;

// Ou via URL
let conn = Connection::establish("http://localhost:8123/default").await?;
let conn = Connection::establish("tcp://localhost:9000/default").await?;

// Même API pour les deux !
conn.execute("CREATE TABLE test (id UInt64) ENGINE = Memory").await?;
```

## HTTP Backend

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
use diesel_clickhouse::Connection;

// Builder pattern (recommandé)
let conn = Connection::http()
    .host("localhost")
    .port(8123)
    .user("default")
    .password("default")
    .database("mydb")
    .build()
    .await?;

// Avec TLS (port 8443)
let conn = Connection::establish("https://localhost:8443/default").await?;
```

### Requêtes et Insertions

```rust
use diesel_clickhouse::prelude::*;

#[row]
#[derive(Debug, Clone)]
struct User {
    id: u64,
    name: String,
}

#[row]
#[derive(Debug, Clone, Insertable)]
#[diesel_clickhouse(table = users)]
struct NewUser {
    id: u64,
    name: String,
}

// Requête avec le query builder
let users: Vec<User> = users::table
    .filter(users::active.eq(true))
    .load(&conn)
    .await?;

// Insertion idiomatique Diesel
insert_into(users::table)
    .values(&[
        NewUser { id: 1, name: "Alice".into() },
        NewUser { id: 2, name: "Bob".into() },
    ])
    .insert(&conn)
    .await?;
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

### Connexion

```rust
use diesel_clickhouse::Connection;

// Builder pattern (recommandé)
let conn = Connection::native()
    .host("localhost")
    .port(9000)
    .user("default")
    .password("default")
    .database("mydb")
    .build()
    .await?;

// Via URL
let conn = Connection::establish("tcp://localhost:9000/default").await?;

// Avec options dans l'URL
let conn = Connection::establish(
    "tcp://admin:secret@clickhouse.example.com:9000/analytics?compression=lz4"
).await?;
```

### Requêtes et Insertions

```rust
use diesel_clickhouse::prelude::*;

#[row]
#[derive(Debug, Clone)]
struct User {
    id: u64,
    name: String,
}

// Même API que HTTP !
let users: Vec<User> = users::table
    .filter(users::active.eq(true))
    .load(&conn)
    .await?;

insert_into(users::table)
    .values(&new_users)
    .insert(&conn)
    .await?;
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
| API unifiée Connection | ✅ | ✅ |
| TLS | ✅ | ✅ |
| Compression | ✅ (via HTTP) | ✅ (LZ4) |
| Connection pooling | ✅ (via Pool) | ✅ (via Pool) |
| Streaming results | ✅ | ✅ |
| Zero-copy Arrow | ✅ | ✅ |
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

---

## Streaming et Arrow

Les deux backends supportent le streaming et Arrow :

```rust
// Streaming - fonctionne avec HTTP et Native
let mut stream = conn
    .stream::<User, _>(users::table.filter(users::active.eq(true)))
    .await?;

while let Some(user) = stream.next().await? {
    println!("User: {}", user.name);
}

// Zero-copy Arrow (HTTP uniquement pour l'instant)
let count = conn.load_zero_copy(
    "SELECT id, name FROM users",
    |row| {
        let name = row.get_str("name")?;  // &str, zero-copy
        Ok(())
    }
).await?;
```

---

## Connection Pooling

Les deux backends supportent le pooling via `Pool` :

```rust
use diesel_clickhouse::{Connection, pool::Pool};

// HTTP pool
let pool = Pool::builder(
    Connection::http()
        .host("localhost")
        .port(8123)
        .database("mydb")
)
.max_size(20)
.min_idle(5)
.build()
.await?;

// Native pool
let pool = Pool::builder(
    Connection::native()
        .host("localhost")
        .port(9000)
        .database("mydb")
)
.max_size(20)
.build()
.await?;

let conn = pool.get().await?;
```

---

## Configuration TLS

### HTTP avec TLS

```rust
use diesel_clickhouse::Connection;

// rustls (recommandé, pur Rust)
// Cargo.toml: features = ["http", "rustls-tls"]
let conn = Connection::establish("https://localhost:8443/default").await?;

// native-tls (utilise OpenSSL)
// Cargo.toml: features = ["http", "native-tls"]
let conn = Connection::establish("https://localhost:8443/default").await?;
```

### Native avec TLS

```rust
use diesel_clickhouse::Connection;

// Cargo.toml: features = ["native", "native-tls-native"]
let conn = Connection::establish(
    "tcp://localhost:9440/default?secure=true"
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
