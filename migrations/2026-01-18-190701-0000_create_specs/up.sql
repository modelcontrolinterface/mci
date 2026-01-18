CREATE TABLE specs (
    id VARCHAR(64) PRIMARY KEY NOT NULL,
    spec_type VARCHAR(64) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    spec_url TEXT NOT NULL,
    source_url TEXT NOT NULL,
    description VARCHAR(500) NOT NULL
);

CREATE INDEX idx_specs_type ON specs(spec_type);
CREATE INDEX idx_specs_enabled ON specs(enabled);
CREATE INDEX idx_specs_description ON specs(description);
