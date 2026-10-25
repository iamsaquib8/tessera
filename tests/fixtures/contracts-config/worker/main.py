import os

DATABASE_URL = os.environ.get("DATABASE_URL")
LEVEL = os.getenv('LOG_LEVEL', 'info')
