-- Bootstrap fixture for the Guthrie, OK demo site.
--
-- This file is NOT a migration. The top-level migrations/ directory is what
-- `sqlx::migrate!` scans; this subdirectory is an editable fixture executed
-- by the `lothal demo-seed` command.
--
-- Contents: one site, one main structure, three utility account shells, one
-- main panel, and a handful of labeled circuits. No readings, no bills, no
-- briefings — those must come from real ingest, not fiction.
--
-- All inserts use ON CONFLICT DO NOTHING on primary key so re-running is safe.

BEGIN;

-- ----- Site ------------------------------------------------------------------
-- Stable UUID so child rows can reference it without a SELECT round-trip.
INSERT INTO sites (
    id, address, city, state, zip,
    latitude, longitude, lot_size_acres, climate_zone, soil_type
) VALUES (
    '11111111-1111-1111-1111-111111111111',
    '2451 N Division St', 'Guthrie', 'OK', '73044',
    35.8986, -97.4254, 2.5, '3A - Warm Humid', 'clay'
) ON CONFLICT (id) DO NOTHING;

-- ----- Structure -------------------------------------------------------------
INSERT INTO structures (
    id, site_id, name, year_built, square_footage, stories,
    foundation_type, has_pool, pool_gallons, has_septic
) VALUES (
    '22222222-2222-2222-2222-222222222222',
    '11111111-1111-1111-1111-111111111111',
    'Main House', 1998, 2400.0, 1,
    'slab', true, 18000.0, true
) ON CONFLICT (id) DO NOTHING;

-- ----- Utility accounts (empty shells, no bills) -----------------------------
INSERT INTO utility_accounts (id, site_id, provider_name, utility_type) VALUES
    ('33333333-3333-3333-3333-333333333301',
     '11111111-1111-1111-1111-111111111111', 'OG&E',            'electric'),
    ('33333333-3333-3333-3333-333333333302',
     '11111111-1111-1111-1111-111111111111', 'ONG',             'gas'),
    ('33333333-3333-3333-3333-333333333303',
     '11111111-1111-1111-1111-111111111111', 'City of Guthrie', 'water')
ON CONFLICT (id) DO NOTHING;

-- ----- Main panel ------------------------------------------------------------
INSERT INTO panels (id, structure_id, name, amperage, is_main) VALUES
    ('44444444-4444-4444-4444-444444444444',
     '22222222-2222-2222-2222-222222222222',
     'Main Panel', 200, true)
ON CONFLICT (id) DO NOTHING;

-- ----- Circuits --------------------------------------------------------------
-- Representative breakers for a 1998 slab-foundation house with pool + septic.
-- Labels mirror what the homeowner would read off their panel door.
INSERT INTO circuits (id, panel_id, breaker_number, label, amperage, is_double_pole) VALUES
    ('55555555-5555-5555-5555-555555555501',
     '44444444-4444-4444-4444-444444444444',  1, 'HVAC Air Handler',  30, false),
    ('55555555-5555-5555-5555-555555555502',
     '44444444-4444-4444-4444-444444444444',  3, 'HVAC Condenser',    40, true),
    ('55555555-5555-5555-5555-555555555503',
     '44444444-4444-4444-4444-444444444444',  5, 'Water Heater',      30, true),
    ('55555555-5555-5555-5555-555555555504',
     '44444444-4444-4444-4444-444444444444',  7, 'Range',             50, true),
    ('55555555-5555-5555-5555-555555555505',
     '44444444-4444-4444-4444-444444444444',  9, 'Dryer',             30, true),
    ('55555555-5555-5555-5555-555555555506',
     '44444444-4444-4444-4444-444444444444', 11, 'Pool Pump',         20, true),
    ('55555555-5555-5555-5555-555555555507',
     '44444444-4444-4444-4444-444444444444', 13, 'Kitchen Outlets',   20, false),
    ('55555555-5555-5555-5555-555555555508',
     '44444444-4444-4444-4444-444444444444', 15, 'Refrigerator',      20, false),
    ('55555555-5555-5555-5555-555555555509',
     '44444444-4444-4444-4444-444444444444', 17, 'Laundry Outlets',   20, false),
    ('55555555-5555-5555-5555-555555555510',
     '44444444-4444-4444-4444-444444444444', 19, 'Lighting - Main',   15, false),
    ('55555555-5555-5555-5555-555555555511',
     '44444444-4444-4444-4444-444444444444', 21, 'Lighting - Bedrooms', 15, false),
    ('55555555-5555-5555-5555-555555555512',
     '44444444-4444-4444-4444-444444444444', 23, 'Septic Pump',       20, false)
ON CONFLICT (id) DO NOTHING;

COMMIT;
