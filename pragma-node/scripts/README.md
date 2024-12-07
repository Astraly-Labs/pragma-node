# Price Publishers

Two simple Python scripts for publishing test price data to a PostgreSQL database. These scripts simulate price feeds for spot and perpetual futures markets.

## Scripts

- `publish_spot.py`: Publishes spot market prices
- `publish_perp.py`: Publishes perpetual futures prices

## Prerequisites

- Python 3.6+
- PostgreSQL database
- Required Python packages:
  ```
  psycopg2-binary
  pytz
  ```

## Installation

1. Clone this repository or download the scripts

2. Install required packages:
   ```bash
   pip install psycopg2-binary pytz
   ```

## Configuration

Set your database URL as an environment variable:
```bash
export OFFCHAIN_DATABASE_URL="postgresql://user:password@host:port/dbname"
```

## Usage

### Spot Price Publisher
```bash
python publish_spot.py
```

This will:
- Connect to the configured PostgreSQL database
- Insert new spot price entries every 500ms
- Use "BTC/USD" as the pair
- Use base price of 9903599000000 with small random deviations
- Set publisher as "AKHERCHA"
- Set source as "TEST"
- Use NULL for publisher signature

### Perpetual Futures Price Publisher
```bash
python publish_perp.py
```

This will:
- Connect to the configured PostgreSQL database
- Insert new perpetual futures price entries every 500ms
- Use "BTC/USD" as the pair
- Use base price of 9903599000000 with small random deviations
- Set publisher as "AKHERCHA"
- Set source as "TEST"
- Set a test signature value
- Use NULL for expiration timestamp

## Data Format

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
