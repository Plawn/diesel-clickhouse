  ---
  🔴 CE QUI PEUT ÊTRE AMÉLIORÉ

  1. Allocations .to_string() Excessives (backend.rs:70-86)

  // backend.rs:70-86
  impl BindableValue {
      pub fn sql_literal(&self) -> String {
          match self {
              Self::UInt8(v) => v.to_string(),
              Self::UInt16(v) => v.to_string(),
              Self::UInt32(v) => v.to_string(),
              // ... 10 allocations par call!
              Self::Bool(v) => if *v { "true".to_string() } else { "false".to_string() },
          }
      }
  }

  Problème : 10+ allocations à chaque appel pour debug/log des valeurs bindées.

  Suggestion : Utiliser Display trait et formatter, ou retourner Cow<'static, str>.

  ---
  2. Clones dans les HashMaps (result.rs:328-330)

  // result.rs:328-330
  for (i, name) in column_names.iter().enumerate() {
      column_index.insert(name.clone(), i);  // Clone à chaque itération!
  }

  Problème : Clone de String pour chaque nom de colonne lors de la construction du ColumnIndex.

  Impact : O(n) allocations pour n colonnes, répété à chaque query.

  ---
  3. Zero-Copy qui n'est pas vraiment Zero-Copy (http.rs:804-833)

  pub async fn load_zero_copy<F>(...) -> QueryResult<usize> {
      let mut all_bytes = Vec::with_capacity(4096);
      loop {
          match cursor.next().await {
              Ok(Some(chunk)) => {
                  all_bytes.extend_from_slice(&chunk);  // Accumule TOUT en mémoire
              }
              ...
          }
      }
      // Parse ENSUITE
      let parser = TsvParser::new(&all_bytes, columns);
  }

  Problème : Accumule tout le résultat en mémoire avant de parser. Pas de vrai streaming.

  Suggestion : Traiter les rows au fur et à mesure des chunks.

  ---
  4. Native Backend ne Stream pas (native.rs + stream.rs)

  // unified.rs:798-806
  pub async fn stream_native<T, Q>(&self, query: Q) -> QueryResult<RowStream<T>> {
      let rows: Vec<T> = self.load_optimized(query).await?;  // Charge TOUT
      Ok(RowStream::from(rows))
  }

  Problème : Le backend Native charge tout le résultat en mémoire via fetch_all(). Limitation de clickhouse-rs.

  Impact : Impossible de streamer des résultats > RAM disponible avec Native.

  ---
  5. for_each_async Séquentiel (stream.rs:126-136)

  pub async fn for_each_async<F, Fut>(mut self, mut f: F) -> QueryResult<()> {
      while let Some(row) = self.next().await? {
          f(row).await;  // Exécution purement séquentielle!
      }
      Ok(())
  }

  Problème : Pas d'option pour paralléliser le traitement des rows.

  Suggestion : Ajouter for_each_async_buffered(buffer_size) avec futures::stream::buffer_unordered().

  ---
  6. Pool Warm-up Séquentiel (pool.rs:299-313)

  for _ in 0..min_idle {
      match pool.inner.factory.create().await {  // Séquentiel
          Ok(conn) => { ... }
          Err(e) => { ... }
      }
  }

  Problème : Connexions créées une par une au démarrage.

  Suggestion : Utiliser futures::future::join_all() pour paralléliser.

  ---
  7. Vec sans Pré-allocation (native.rs)

  // native.rs:254
  let mut params = Vec::new();  // Devrait être Vec::with_capacity(8)

  // native.rs:549+
  fn new_column() -> Self::ColumnData {
      Vec::new()  // Devrait avoir une capacité par défaut
  }

  Impact : Réallocations multiples pour Vec qui grandissent.

  ---
  8. std::sync::Mutex dans Async Context (async_insert.rs:306)

  pub struct BufferedAsyncInserter<'a, T, R> {
      buffer: std::sync::Mutex<Vec<R>>,  // SYNC Mutex dans async!
  }

  Problème potentiel : Peut bloquer le thread tokio (section critique courte, donc impact limité).

  Suggestion : Utiliser tokio::sync::Mutex pour cohérence.

  ---
  9. Messages d'Erreur avec Allocations (native.rs, unified.rs)

  Error::ConnectionError("Missing host in URL".to_string())
  Error::QueryError(format!("Invalid value: {}", e))

  Problème : ~20+ occurrences de .to_string() pour les erreurs.

  Suggestion : Utiliser Cow<'static, str> ou des erreurs avec &'static str quand possible.

  ---
  📊 Tableau Comparatif des Backends

  | Aspect              | HTTP                   | Native            |
  |---------------------|------------------------|-------------------|
  | Streaming           | ✅ Vrai (O(1) mémoire) | ❌ Buffé (O(n))   |
  | Format              | RowBinary/JSON/TSV     | Binaire natif     |
  | Performance parsing | 2-3x JSON              | 2-3x JSON         |
  | Large datasets      | ✅ Excellent           | ⚠️ Limité par RAM |
  | Latence             | HTTP keep-alive        | TCP pool          |
  | Zero-copy           | ✅ Via TSV             | ❌ Non disponible |

  ---
  📈 Recommandations Prioritaires

  Haute Priorité

  1. Refactorer load_zero_copy pour traiter les rows en streaming réel
  2. Ajouter with_capacity aux Vec dans native.rs (lignes 254, 549+)
  3. Remplacer .to_string() dans BindableValue::sql_literal() par Display/Cow

  Moyenne Priorité

  4. Ajouter for_each_async_buffered pour parallélisation optionnelle
  5. Paralléliser le pool warm-up avec join_all()
  6. Utiliser Cow<'static, str> pour les messages d'erreur statiques

  Basse Priorité

  7. Remplacer std::sync::Mutex par tokio::sync::Mutex dans BufferedAsyncInserter
  8. Considérer simd-json pour le parsing JSON en fallback

  ---
  🏆 Conclusion

  Le projet diesel-clickhouse est très bien optimisé avec des techniques avancées :

  - ✅ Arena allocation avec bumpalo
  - ✅ String interning avec fast path
  - ✅ Zero-copy parsing avec SmallVec
  - ✅ Format RowBinary pour éviter JSON
  - ✅ itoa/ryu pour numériques

  Les axes d'amélioration concernent principalement :

  - ⚠️ Le backend Native qui ne stream pas vraiment
  - ⚠️ Quelques allocations évitables dans les chemins chauds
  - ⚠️ La fonction load_zero_copy qui accumule avant de parser

  Score global : 8.5/10 - Production-ready avec d'excellentes performances pour la majorité des cas d'usage.