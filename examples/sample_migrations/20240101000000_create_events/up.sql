-- Create the events table
CREATE TABLE IF NOT EXISTS events (
    id UInt64,
    user_id UInt32,
    event_type LowCardinality(String),
    timestamp DateTime64(3) DEFAULT now64(3),
    value Float64 DEFAULT 0.0,
    properties Map(String, String) DEFAULT map()
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (user_id, timestamp, id);
