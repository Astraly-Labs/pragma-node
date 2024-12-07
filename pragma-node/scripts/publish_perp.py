import os
import time
import random
import psycopg2
from datetime import datetime
import pytz

# Get database URL from environment variable
DATABASE_URL = os.getenv('OFFCHAIN_DATABASE_URL')

def insert_future_entry(conn):
    # Base price with small random deviation
    base_price = 9903599000000
    price = base_price + random.randint(-1000000, 1000000)
    
    # Current timestamp in seconds
    timestamp = datetime.now(pytz.UTC)
    
    # Insert query
    query = """
        INSERT INTO future_entries (pair_id, price, timestamp, expiration_timestamp, 
                                  publisher, publisher_signature, source)
        VALUES (%s, %s, %s, %s, %s, %s, %s)
    """
    
    with conn.cursor() as cur:
        cur.execute(query, (
            "BTC/USD",
            price,
            timestamp,
            None,  # expiration_timestamp
            "AKHERCHA",
            "TEST_SIGNATURE",  # Note: future entries require a signature
            "TEST"
        ))
        conn.commit()

def main():
    # Connect to the database
    conn = psycopg2.connect(DATABASE_URL)
    count = 0
    try:
        print("Starting future entry insertion...")
        while True:
            insert_future_entry(conn)
            count += 1
            print(f"\rInserted {count} entries...", end="")
            time.sleep(0.5)  # Wait 500ms
    except KeyboardInterrupt:
        print("\nStopping future insertion...")
    finally:
        conn.close()

if __name__ == "__main__":
    main()
