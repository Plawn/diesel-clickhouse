⏺ Tu as raison de poser la question. Actuellement il y a 3 formats utilisés:

  | Format       | Méthodes                        | Usage                                                  |
  |--------------|---------------------------------|--------------------------------------------------------|
  | RowBinary    | load_binary(), first_binary()   | Le plus rapide, nécessite #[derive(Row)] de clickhouse |
  | JSONEachRow  | load(), load_native(), stream() | Pour types avec #[derive(Queryable)] (serde)           |
  | TabSeparated | load_zero_copy()                | Zero-copy parsing                                      |

  Si tu utilises uniquement load_binary() avec le derive Row de clickhouse, alors simd-json n'est plus nécessaire.

  Veux-tu:
  1. Supprimer simd-json et les méthodes JSON (garder uniquement RowBinary) ?