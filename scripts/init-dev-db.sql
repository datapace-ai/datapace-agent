-- Development database initialization script
-- This script sets up a local PostgreSQL for testing the Datapace Agent

-- Enable pg_stat_statements extension
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

-- Create sample schema for testing
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS orders (
    id SERIAL PRIMARY KEY,
    user_id INTEGER REFERENCES users(id),
    total_amount DECIMAL(10, 2) NOT NULL,
    status VARCHAR(50) DEFAULT 'pending',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS order_items (
    id SERIAL PRIMARY KEY,
    order_id INTEGER REFERENCES orders(id),
    product_name VARCHAR(255) NOT NULL,
    quantity INTEGER NOT NULL,
    unit_price DECIMAL(10, 2) NOT NULL
);

-- Create indexes for testing
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_orders_user_id ON orders(user_id);
CREATE INDEX IF NOT EXISTS idx_orders_status ON orders(status);
CREATE INDEX IF NOT EXISTS idx_order_items_order_id ON order_items(order_id);

-- Insert sample data
INSERT INTO users (email, name) VALUES
    ('alice@example.com', 'Alice Johnson'),
    ('bob@example.com', 'Bob Smith'),
    ('charlie@example.com', 'Charlie Brown')
ON CONFLICT (email) DO NOTHING;

INSERT INTO orders (user_id, total_amount, status) VALUES
    (1, 99.99, 'completed'),
    (1, 149.99, 'pending'),
    (2, 299.99, 'completed'),
    (3, 49.99, 'shipped');

INSERT INTO order_items (order_id, product_name, quantity, unit_price) VALUES
    (1, 'Widget A', 2, 29.99),
    (1, 'Widget B', 1, 39.99),
    (2, 'Gadget X', 1, 149.99),
    (3, 'Widget A', 5, 29.99),
    (3, 'Widget C', 2, 74.99),
    (4, 'Gadget Y', 1, 49.99);

-- Run some queries to populate pg_stat_statements
SELECT * FROM users WHERE email = 'alice@example.com';
SELECT u.name, COUNT(o.id) as order_count, SUM(o.total_amount) as total_spent
FROM users u
LEFT JOIN orders o ON u.id = o.user_id
GROUP BY u.id, u.name;
SELECT * FROM orders WHERE status = 'completed';

-- Grant permissions to datapace user (if exists)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'datapace') THEN
        GRANT pg_read_all_stats TO datapace;
        GRANT USAGE ON SCHEMA public TO datapace;
        GRANT SELECT ON ALL TABLES IN SCHEMA public TO datapace;
    END IF;
END
$$;

-- Analyze tables
ANALYZE users;
ANALYZE orders;
ANALYZE order_items;
