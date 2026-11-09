# Jocasta's Graphiti service (5.9, 0334). Built by `src/shared/containers`; the base image is pinned by digest (0074),
# every Python package by version and hash (0430: requirements.txt, PyTorch's CPU build from its own index so
# sentence-transformers doesn't pull the CUDA wheels), and the embedding model by its Hugging Face revision.
FROM python:3.12-slim@sha256:05cda9777409a9c3ffddd94a4c476b79f0769a0b4857f0c7ed9226b6800b0d6f

ENV PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    GRAPHITI_TELEMETRY_ENABLED=false \
    HF_HOME=/opt/hf \
    EMBEDDING_MODEL=sentence-transformers/all-MiniLM-L6-v2 \
    EMBEDDING_REVISION=1110a243fdf4706b3f48f1d95db1a4f5529b4d41 \
    EMBEDDING_DIM=384

COPY requirements.txt /app/requirements.txt
RUN pip install --require-hashes --no-deps -r /app/requirements.txt \
    --index-url https://pypi.org/simple --extra-index-url https://download.pytorch.org/whl/cpu

# The service runs as an unprivileged user (0484), which owns only the model cache.
RUN useradd --uid 10001 --create-home --home-dir /home/graphiti --shell /usr/sbin/nologin graphiti \
    && mkdir /opt/hf && chown 10001:10001 /opt/hf
USER 10001

# The embedding model is baked in at its pinned revision; the running service never downloads anything (0484: telemetry
# is off and Hugging Face is offline, so its only calls out are to the companion's model endpoint).
RUN python -c "import os; from sentence_transformers import SentenceTransformer; SentenceTransformer(os.environ['EMBEDDING_MODEL'], revision=os.environ['EMBEDDING_REVISION'])"
ENV HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1

COPY service.py /app/service.py
EXPOSE 8000
CMD ["python", "/app/service.py"]
