-- Add is_public field to captures table
-- This field allows users to choose if their capture appears in the public feed (stories)

ALTER TABLE captures 
ADD COLUMN is_public BOOLEAN NOT NULL DEFAULT false;

-- Add index for filtering public captures
CREATE INDEX idx_captures_is_public ON captures(is_public) WHERE is_public = true;

-- Add comment explaining the field
COMMENT ON COLUMN captures.is_public IS 'User preference: if true, capture will be published to the public stories feed';
