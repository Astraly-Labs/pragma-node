-- Your SQL goes here
ALTER TABLE entries 
SET (
    timescaledb.compress, 
    timescaledb.compress_segmentby='pair_id, source', 
    timescaledb.compress_orderby='timestamp DESC, id'
);
SELECT add_compression_policy('entries', INTERVAL '30 days');