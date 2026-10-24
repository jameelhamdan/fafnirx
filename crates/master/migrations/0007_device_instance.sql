-- 0007: one computer at a time per device. The token says which device; the instance id says which installation of the
-- client is using it. The first one to connect holds the device, and another one is refused for as long as the first is
-- heard from (see devices/grpc.rs). Rotating the token, or releasing the device in the web app, frees it.
ALTER TABLE devices ADD COLUMN instance_id TEXT;
