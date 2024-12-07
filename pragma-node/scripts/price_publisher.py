# price_publisher.py
import os
import sys
import time
import random
import logging
from typing import Literal
import psycopg2
from datetime import datetime
import click
import pytz
from contextlib import contextmanager

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

class DatabaseError(Exception):
    """Custom exception for database-related errors"""
    pass

@contextmanager
def get_db_connection(db_url: str):
    """Context manager for database connections"""
    conn = None
    try:
        conn = psycopg2.connect(db_url)
        yield conn
    except psycopg2.Error as e:
        logger.error(f"Database connection error: {e}")
        raise DatabaseError(f"Failed to connect to database: {e}")
    finally:
        if conn is not None:
            conn.close()
            logger.debug("Database connection closed")

class PricePublisher:
    def __init__(
        self, 
        db_url: str,
        market_type: Literal['spot', 'perp'],
        pair_id: str,
        base_price: int,
        num_publishers: int,
        interval: float = 0.5
    ):
        self.db_url = db_url
        self.market_type = market_type
        self.pair_id = pair_id
        self.base_price = base_price
        self.num_publishers = num_publishers
        self.interval = interval
        
        # Validate inputs
        self._validate_inputs()
        
    def _validate_inputs(self):
        """Validate initialization parameters"""
        if self.base_price <= 0:
            raise ValueError("Base price must be positive")
        if self.num_publishers <= 0:
            raise ValueError("Number of publishers must be positive")
        if self.interval <= 0:
            raise ValueError("Interval must be positive")

    def _generate_price(self) -> int:
        """Generate a price with random deviation"""
        deviation = random.uniform(-0.001, 0.001)  # ±0.1% deviation
        return int(self.base_price * (1 + deviation))

    def _insert_spot_entry(self, conn, publisher: str):
        """Insert a spot market entry"""
        query = """
            INSERT INTO entries (pair_id, publisher, timestamp, price, source, publisher_signature)
            VALUES (%s, %s, %s, %s, %s, %s)
        """
        with conn.cursor() as cur:
            cur.execute(query, (
                self.pair_id,
                publisher,
                datetime.now(pytz.UTC),
                self._generate_price(),
                "TEST",
                None
            ))
            conn.commit()

    def _insert_perp_entry(self, conn, publisher: str):
        """Insert a perpetual futures entry"""
        query = """
            INSERT INTO future_entries (
                pair_id, price, timestamp, expiration_timestamp, 
                publisher, publisher_signature, source
            )
            VALUES (%s, %s, %s, %s, %s, %s, %s)
        """
        with conn.cursor() as cur:
            cur.execute(query, (
                self.pair_id,
                self._generate_price(),
                datetime.now(pytz.UTC),
                None,
                publisher,
                f"TEST_SIGNATURE_{publisher}",
                "TEST"
            ))
            conn.commit()

    def _get_publisher_names(self) -> list[str]:
        """Generate list of publisher names"""
        return [f"ADEL{i}" for i in range(self.num_publishers)]

    def run(self):
        """Main execution loop"""
        logger.info(f"Starting {self.market_type} price publisher")
        logger.info(f"Publishing for pair: {self.pair_id}")
        logger.info(f"Number of publishers: {self.num_publishers}")
        logger.info(f"Base price: {self.base_price}")
        logger.info(f"Update interval: {self.interval}s")

        publishers = self._get_publisher_names()
        insert_func = self._insert_perp_entry if self.market_type == 'perp' else self._insert_spot_entry

        try:
            with get_db_connection(self.db_url) as conn:
                while True:
                    start_time = time.time()
                    
                    for publisher in publishers:
                        try:
                            insert_func(conn, publisher)
                            logger.debug(f"Published price for {publisher}")
                        except psycopg2.Error as e:
                            logger.error(f"Error publishing price for {publisher}: {e}")
                            continue

                    # Maintain consistent interval accounting for processing time
                    elapsed = time.time() - start_time
                    sleep_time = max(0, self.interval - elapsed)
                    time.sleep(sleep_time)

        except KeyboardInterrupt:
            logger.info("Shutting down price publisher...")
        except DatabaseError as e:
            logger.error(f"Fatal database error: {e}")
            sys.exit(1)
        except Exception as e:
            logger.error(f"Unexpected error: {e}")
            sys.exit(1)

@click.command()
@click.option(
    '--market-type',
    type=click.Choice(['spot', 'perp']),
    required=True,
    help='Market type to publish prices for'
)
@click.option(
    '--pair',
    required=True,
    help='Trading pair to publish prices for (e.g., BTC/USD)'
)
@click.option(
    '--base-price',
    type=int,
    required=True,
    help='Base price to use (will have small deviations)'
)
@click.option(
    '--publishers',
    type=int,
    default=1,
    help='Number of publishers to simulate'
)
@click.option(
    '--interval',
    type=float,
    default=0.5,
    help='Publishing interval in seconds'
)
@click.option(
    '--debug',
    is_flag=True,
    help='Enable debug logging'
)
def main(market_type: str, pair: str, base_price: int, publishers: int, interval: float, debug: bool):
    """Price publisher for spot and perpetual futures markets"""
    if debug:
        logger.setLevel(logging.DEBUG)

    db_url = os.getenv('OFFCHAIN_DATABASE_URL')
    if not db_url:
        logger.error("OFFCHAIN_DATABASE_URL environment variable not set")
        sys.exit(1)

    try:
        publisher = PricePublisher(
            db_url=db_url,
            market_type=market_type,
            pair_id=pair,
            base_price=base_price,
            num_publishers=publishers,
            interval=interval
        )
        publisher.run()
    except ValueError as e:
        logger.error(f"Configuration error: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()
