ALTER TABLE buildings ADD COLUMN production_status INTEGER NOT NULL DEFAULT 1 CHECK (production_status >= 0);
ALTER TABLE buildings ADD COLUMN recipe_id INTEGER NOT NULL DEFAULT 0 CHECK (recipe_id >= 0);
ALTER TABLE buildings ADD COLUMN item_count INTEGER NOT NULL DEFAULT 0 CHECK (item_count >= 0);
ALTER TABLE buildings ADD COLUMN product_count INTEGER NOT NULL DEFAULT 0 CHECK (product_count >= 0);
ALTER TABLE buildings ADD COLUMN last_update_at INTEGER NOT NULL DEFAULT 0 CHECK (last_update_at >= 0);
ALTER TABLE buildings ADD COLUMN recipe_time INTEGER NOT NULL DEFAULT 0 CHECK (recipe_time >= 0);
ALTER TABLE buildings ADD COLUMN productivity INTEGER NOT NULL DEFAULT 0 CHECK (productivity >= 0);
ALTER TABLE buildings ADD COLUMN produce_speed INTEGER NOT NULL DEFAULT 0 CHECK (produce_speed >= 0);
