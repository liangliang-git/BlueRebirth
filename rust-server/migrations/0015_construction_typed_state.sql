ALTER TABLE construction_jobs ADD COLUMN duration_seconds INTEGER NOT NULL DEFAULT 0 CHECK (duration_seconds >= 0);
ALTER TABLE construction_jobs ADD COLUMN project_gold INTEGER NOT NULL DEFAULT 0 CHECK (project_gold >= 0);
ALTER TABLE construction_jobs ADD COLUMN project_steel INTEGER NOT NULL DEFAULT 0 CHECK (project_steel >= 0);
ALTER TABLE construction_jobs ADD COLUMN project_aluminium INTEGER NOT NULL DEFAULT 0 CHECK (project_aluminium >= 0);
ALTER TABLE construction_jobs ADD COLUMN completed INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1));
