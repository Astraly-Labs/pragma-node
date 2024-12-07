import os
import time
import random
import psycopg2
from datetime import datetime
import pytz

# Get database URL from environment variable
DATABASE_URL = os.getenv('OFFCHAIN_DATABASE_URL')

def insert_spot_entry(conn):
    # Base price with small random deviation
    base_price = 9903599000000
    price = base_price + random.randint(-1000000, 1000000)
    
    # Current timestamp in seconds
    timestamp = datetime.now(pytz.UTC)
    
    # Insert query
    query = """
        INSERT INTO entries (pair_id, publisher, timestamp, price, source, publisher_signature)
        VALUES (%s, %s, %s, %s, %s, %s)
    """
    
    with conn.cursor() as cur:
        cur.execute(query, (
            "BTC/USD",
            "AKHERCHA",
            timestamp,
            price,
            "TEST",
            "TEST_SIGNATURE"
        ))
        conn.commit()

def main():
    # Connect to the database
    conn = psycopg2.connect(DATABASE_URL)
    count = 0
    try:
        print("Starting spot entry insertion...")
        while True:
            insert_spot_entry(conn)
            count += 1
            print(f"\rInserted {count} entries...", end="")
            time.sleep(0.5)  # Wait 500ms
    except KeyboardInterrupt:
        print("\nStopping spot insertion...")
    finally:
        conn.close()

if __name__ == "__main__":
    main()
