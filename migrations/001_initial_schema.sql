-- CrazyTrip Crazydex Capture Service - Database Migrations
-- Version: 1.0.0

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Captures table
CREATE TABLE IF NOT EXISTS captures (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID,
    device_local_id VARCHAR(255),
    image_url TEXT NOT NULL,
    thumbnail_url TEXT,
    image_size BIGINT,
    storage_type VARCHAR(50) NOT NULL DEFAULT 's3',
    vision_result JSONB,
    category VARCHAR(100),
    confidence DOUBLE PRECISION,
    tags TEXT[],
    location JSONB,
    location_info JSONB,
    orientation JSONB,
    is_deleted BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for captures
CREATE INDEX IF NOT EXISTS idx_captures_user_id ON captures(user_id);
CREATE INDEX IF NOT EXISTS idx_captures_created_at ON captures(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_captures_category ON captures(category);
CREATE INDEX IF NOT EXISTS idx_captures_device_local_id ON captures(device_local_id);
CREATE INDEX IF NOT EXISTS idx_captures_is_deleted ON captures(is_deleted);
CREATE INDEX IF NOT EXISTS idx_captures_vision_result ON captures USING GIN (vision_result);

-- Analysis results table
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
CREATE INDEX IF NOT EXISTS idx_analysis_results_created_at ON analysis_results(created_at DESC);

-- Device uploads tracking
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
CREATE INDEX IF NOT EXISTS idx_device_uploads_server_capture_id ON device_uploads(server_capture_id);

-- Analysis queue (for background worker)
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
CREATE INDEX IF NOT EXISTS idx_analysis_queue_capture_id ON analysis_queue(capture_id);

-- Optional: Tags table (normalized)
CREATE TABLE IF NOT EXISTS tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) UNIQUE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);

-- Optional: Capture-Tags junction table
CREATE TABLE IF NOT EXISTS capture_tags (
    capture_id UUID NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (capture_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_capture_tags_capture_id ON capture_tags(capture_id);
CREATE INDEX IF NOT EXISTS idx_capture_tags_tag_id ON capture_tags(tag_id);

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for captures table
CREATE TRIGGER update_captures_updated_at
BEFORE UPDATE ON captures
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();

-- Comments for documentation
COMMENT ON TABLE captures IS 'Stores image captures with metadata and AI analysis results';
COMMENT ON TABLE analysis_results IS 'Historical analysis results with model versions';
COMMENT ON TABLE device_uploads IS 'Tracks sync status from mobile devices';
COMMENT ON TABLE analysis_queue IS 'Queue for pending AI analysis tasks';
COMMENT ON COLUMN captures.vision_result IS 'JSON result from Gemini Vision API';
COMMENT ON COLUMN captures.location IS 'JSON with latitude/longitude';
COMMENT ON COLUMN captures.location_info IS 'JSON with geocoding information';
COMMENT ON COLUMN captures.orientation IS 'JSON with camera bearing/pitch';
