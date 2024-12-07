# Unified Price Publisher

A robust Python application for simulating price feeds from multiple publishers for both spot and perpetual futures markets.

## Features

- Support for both spot and perpetual futures markets
- Multiple simultaneous publishers
- Configurable trading pairs
- Configurable base price with random deviations
- Adjustable publishing interval
- Proper error handling and logging
- Command-line interface using Click

## Prerequisites

- Python 3.6+
- PostgreSQL database
- Required Python packages:
  ```
  psycopg2-binary
  pytz
  click
  ```

## Installation

1. Clone this repository or download the script

2. Install required packages:
   ```bash
   pip install psycopg2-binary pytz click
   ```

## Configuration

Set your database URL as an environment variable:
```bash
export OFFCHAIN_DATABASE_URL="postgresql://user:password@host:port/dbname"
```

## Usage

### Command Line Interface

```bash
python price_publisher.py [OPTIONS]
```

#### Required Options:
- `--market-type [spot|perp]`: Type of market to publish prices for
- `--pair TEXT`: Trading pair to publish prices for (e.g., BTC/USD)
- `--base-price INTEGER`: Base price to use (will have small deviations)

#### Optional Options:
- `--publishers INTEGER`: Number of publishers to simulate (default: 1)
- `--interval FLOAT`: Publishing interval in seconds (default: 0.5)
- `--debug`: Enable debug logging
- `--help`: Show help message

### Examples

1. Publish spot prices for BTC/USD with 5 publishers:
   ```bash
   python price_publisher.py --market-type spot --pair BTC/USD --base-price 9903599000000 --publishers 5
   ```

2. Publish perpetual futures prices with custom interval:
   ```bash
   python price_publisher.py --market-type perp --pair ETH/USD --base-price 1234567000000 --publishers 3 --interval 1.0
   ```

3. Run with debug logging:
   ```bash
   python price_publisher.py --market-type spot --pair BTC/USD --base-price 9903599000000 --debug
   ```

## Price Deviation

The publisher implements a random price deviation of ±0.1% from the base price for each publication to simulate market movements.

## Error Handling

The application includes comprehensive error handling for:
- Database connection issues
- Invalid configuration values
- Runtime errors
- Graceful shutdown on keyboard interrupt

## Logging

- Default logging level is INFO
- Debug logging available with `--debug` flag
- Logs include timestamps and error details
- Console output for monitoring publication status

## Database Schema

### Spot Entries Table
```sql
CREATE TABLE public.entries (
    id uuid NOT NULL DEFAULT uuid_generate_v4(),
    pair_id character varying NOT NULL,
    publisher text NOT NULL,
    timestamp timestamp with time zone NOT NULL,
    price numeric NOT NULL,
    source character varying NOT NULL,
    publisher_signature character varying NULL
);
```

### Perpetual Futures Entries Table
```sql
CREATE TABLE public.future_entries (
    id uuid NOT NULL DEFAULT uuid_generate_v4(),
    pair_id character varying NOT NULL,
    price numeric NOT NULL,
    timestamp timestamp with time zone NOT NULL,
    expiration_timestamp timestamp with time zone NULL,
    publisher text NOT NULL,
    publisher_signature text NOT NULL,
    source character varying NOT NULL
);
```
