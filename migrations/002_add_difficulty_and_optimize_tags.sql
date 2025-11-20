-- Migration 002: Add difficulty field and optimize tags system
-- Version: 1.1.0
-- Date: 2025-11-19

-- Add difficulty column to captures table with default value
ALTER TABLE captures ADD COLUMN IF NOT EXISTS difficulty VARCHAR(50) DEFAULT 'MEDIUM';

-- Update existing NULL values to default
UPDATE captures SET difficulty = 'MEDIUM' WHERE difficulty IS NULL;

-- Create index for difficulty queries
CREATE INDEX IF NOT EXISTS idx_captures_difficulty ON captures(difficulty);

-- Ensure tags table indexes exist (should be from 001 but we ensure them)
CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);

-- Ensure capture_tags junction table indexes exist
CREATE INDEX IF NOT EXISTS idx_capture_tags_capture_id ON capture_tags(capture_id);
CREATE INDEX IF NOT EXISTS idx_capture_tags_tag_id ON capture_tags(tag_id);

-- Add composite index for tag queries
CREATE INDEX IF NOT EXISTS idx_capture_tags_composite ON capture_tags(capture_id, tag_id);

-- Add comment for difficulty column
COMMENT ON COLUMN captures.difficulty IS 'Difficulty level to capture this item: EASY/MEDIUM/HARD/EXPERT';

-- Function to get all tags for a capture (utility view)
CREATE OR REPLACE VIEW capture_tags_view AS
SELECT 
    c.id as capture_id,
    c.category,
    c.difficulty,
    ARRAY_AGG(t.name ORDER BY t.name) as tags
FROM captures c
LEFT JOIN capture_tags ct ON c.id = ct.capture_id
LEFT JOIN tags t ON ct.tag_id = t.id
WHERE c.is_deleted = false
GROUP BY c.id, c.category, c.difficulty;

COMMENT ON VIEW capture_tags_view IS 'Convenience view showing captures with their aggregated tags';
