-- Add verified field to captures table
-- This field indicates if the AI has verified the geographic authenticity
-- of the captured object/animal/place based on GPS location and knowledge

ALTER TABLE captures 
ADD COLUMN verified BOOLEAN DEFAULT false;

-- Add index for filtering verified captures
CREATE INDEX idx_captures_verified ON captures(verified) WHERE verified = true;

-- Update existing captures to false (will be re-analyzed)
UPDATE captures SET verified = false WHERE verified IS NULL;

-- Add comment explaining the field
COMMENT ON COLUMN captures.verified IS 'AI-verified geographic authenticity: true only if object/animal naturally exists at GPS location or in known zoo/sanctuary at that location';
