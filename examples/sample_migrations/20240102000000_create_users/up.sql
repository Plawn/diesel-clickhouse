-- Create the users table
CREATE TABLE IF NOT EXISTS users (
    id UInt64,
    email String,
    name String,
    country LowCardinality(String) DEFAULT '',
    created_at DateTime DEFAULT now(),
    updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at)
ORDER BY id;

-- Create index on email
ALTER TABLE users ADD INDEX idx_email email TYPE bloom_filter GRANULARITY 1;
