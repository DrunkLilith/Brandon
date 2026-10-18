FROM nvidia/cuda:12.8.1-base-ubuntu24.04@sha256:133c78a0575303be34164d0b90137a042172bdf60696af01a3c424ab402d86e2 AS ocr
ENV DEBIAN_FRONTEND=noninteractive \
    HF_HUB_DISABLE_TELEMETRY=1 HF_HUB_DISABLE_IMPLICIT_TOKEN=1 DO_NOT_TRACK=1 \
    HF_HOME=/cache/huggingface PYTHONUNBUFFERED=1 PYTHONDONTWRITEBYTECODE=1 \
    OMP_NUM_THREADS=4 PATH=/opt/venv/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 python3-venv ca-certificates ffmpeg espeak-ng \
    tesseract-ocr tesseract-ocr-rus tesseract-ocr-eng fonts-dejavu-core libgl1 libglib2.0-0 \
    && rm -rf /var/lib/apt/lists/* \
    && python3 -m venv /opt/venv
COPY requirements-ocr.txt /opt/requirements-ocr.txt
RUN pip install --no-cache-dir -r /opt/requirements-ocr.txt \
    && pip freeze > /opt/requirements-ocr.resolved.txt \
    && dpkg-query -W > /opt/packages.resolved.txt
WORKDIR /work
ENTRYPOINT ["python"]

FROM ocr AS asr
RUN pip install --no-cache-dir torch==2.8.0 --index-url https://download.pytorch.org/whl/cu128
COPY requirements-asr.txt /opt/requirements-asr.txt
RUN pip install --no-cache-dir -r /opt/requirements-asr.txt \
    && pip freeze > /opt/requirements-asr.resolved.txt
