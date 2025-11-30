-- V0002__add_tags_and_missing_columns.sql
-- Add tags tables and missing capture columns expected by the application

-- Add missing columns to captures (if not present)
ALTER TABLE captures ADD COLUMN IF NOT EXISTS difficulty VARCHAR(50);
ALTER TABLE captures ADD COLUMN IF NOT EXISTS verified BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE captures ADD COLUMN IF NOT EXISTS is_public BOOLEAN NOT NULL DEFAULT false;

-- Ensure tags table exists
CREATE TABLE IF NOT EXISTS tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT UNIQUE NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);

-- Join table between captures and tags
CREATE TABLE IF NOT EXISTS capture_tags (
    capture_id UUID NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (capture_id, tag_id)
);
CREATE INDEX IF NOT EXISTS idx_capture_tags_capture_id ON capture_tags(capture_id);
CREATE INDEX IF NOT EXISTS idx_capture_tags_tag_id ON capture_tags(tag_id);

-- Ensure analysis_results, device_uploads and analysis_queue exist (idempotent)
CREATE TABLE IF NOT EXISTS analysis_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    capture_id UUID NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    model_name VARCHAR(100) NOT NULL,
    model_version VARCHAR(50) NOT NULL,
    result JSONB NOT NULL,
    confidence DOUBLE PRECISION,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_analysis_results_capture_id ON analysis_results(capture_id);

CREATE TABLE IF NOT EXISTS device_uploads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id VARCHAR(255) NOT NULL,
    device_local_id VARCHAR(255) NOT NULL,
    server_capture_id UUID REFERENCES captures(id) ON DELETE SET NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    error_message TEXT,
    last_attempt TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(device_id, device_local_id)
);
CREATE INDEX IF NOT EXISTS idx_device_uploads_device_id ON device_uploads(device_id);
CREATE INDEX IF NOT EXISTS idx_device_uploads_status ON device_uploads(status);

CREATE TABLE IF NOT EXISTS analysis_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    capture_id UUID NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_attempt TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_analysis_queue_status ON analysis_queue(status);
CREATE INDEX IF NOT EXISTS idx_analysis_queue_created_at ON analysis_queue(created_at);
