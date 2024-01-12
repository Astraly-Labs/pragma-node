-- This file should undo anything in `up.sql`
ALTER TABLE entries 
SET (
    timescaledb.compress = false
);

SELECT remove_compression_policy('entries');