import os

import uvicorn

uvicorn.run(
    "fafnirx_web.app:create_app",
    factory=True,
    host=os.environ.get("FAFNIRX_WEB_HOST", "0.0.0.0"),
    port=int(os.environ.get("FAFNIRX_WEB_PORT", "8080")),
    proxy_headers=True,
)
