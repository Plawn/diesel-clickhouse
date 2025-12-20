CREATE TABLE IF NOT EXISTS posts (
    id UInt64,
    user_id UInt64,
    title String,
    content String,
    published Bool DEFAULT false,
    created_at DateTime DEFAULT now()
) ENGINE = MergeTree()
ORDER BY (id, created_at)
