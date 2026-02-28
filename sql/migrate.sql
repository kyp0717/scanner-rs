CREATE TABLE IF NOT EXISTS sightings (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    symbol text UNIQUE NOT NULL,
    first_seen timestamptz NOT NULL,
    last_seen timestamptz NOT NULL,
    scanners text NOT NULL,              -- comma-separated list
    hit_count integer DEFAULT 1,
    last_price float8,
    change_pct float8,
    rvol float8,
    float_shares float8,
    catalyst text,                       -- news headline
    name text,
    sector text
);

ALTER TABLE sightings ENABLE ROW LEVEL SECURITY;

CREATE POLICY "Allow all for anon" ON sightings
    FOR ALL
    TO anon
    USING (true)
    WITH CHECK (true);
