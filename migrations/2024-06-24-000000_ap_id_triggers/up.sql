ALTER TABLE comment
    ALTER COLUMN ap_id DROP DEFAULT;

ALTER TABLE post
    ALTER COLUMN ap_id DROP DEFAULT;

ALTER TABLE private_message
    ALTER COLUMN ap_id DROP DEFAULT;

