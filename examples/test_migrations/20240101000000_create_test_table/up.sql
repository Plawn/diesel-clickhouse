-- Create test table with int, string, and JSON (stored as String in ClickHouse)
CREATE TABLE IF NOT EXISTS test_items (
    id UInt64,
    name String,
    metadata String,  -- JSON stored as String
    created_at DateTime DEFAULT now()
) ENGINE = MergeTree()
ORDER BY id;
