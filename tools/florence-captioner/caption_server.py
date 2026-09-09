"""
Florence-2 Local Captioning Server

A lightweight HTTP server that uses Microsoft Florence-2 for image captioning.
Provides a /caption endpoint compatible with the Rust FlorenceCaptioner client.

Usage:
    python caption_server.py [--port 1433] [--model Florence-2-base]

Requirements:
    pip install torch transformers flask pillow
"""

import argparse
import base64
import io
import json
import sys
import time
from http.server import HTTPServer, BaseHTTPRequestHandler

try:
    import torch
    from transformers import AutoProcessor, AutoModelForCausalLM
    from PIL import Image
except ImportError as e:
    print(f"Missing dependency: {e}")
    print("Install with: pip install torch transformers pillow")
    sys.exit(1)


# ── Configuration ─────────────────────────────────────────────────

DEFAULT_PORT = 1433
DEFAULT_MODEL = "microsoft/Florence-2-base"

# Prompts for Florence-2
CAPTION_PROMPT = "<CAPTION>"           # Short caption
DETAILED_PROMPT = "<DETAILED_CAPTION>"  # Detailed caption
OD_PROMPT = "<OD>"                      # Object detection
OCR_PROMPT = "<OCR>"                    # Text recognition

# Structured analysis prompt for RAG pipeline
ANALYSIS_PROMPT_TEMPLATE = """请详细描述这张图片，以 JSON 格式输出（不要输出其他内容）：
{{
  "caption": "详细自然语言描述（2-4句话，描述图片的主要内容、场景、风格）",
  "tags": [
    {{"name": "标签名", "confidence": 0.9}}
  ],
  "entities": [
    {{"type": "character|object|location|style|action", "name": "实体名", "confidence": 0.9}}
  ],
  "ocr_text": "图片中的文字（如果没有则为null）"
}}"""


class Florence2Captioner:
    """Florence-2 model wrapper."""

    def __init__(self, model_name: str, device: str = "auto"):
        self.model_name = model_name
        self.device = self._resolve_device(device)
        self.model = None
        self.processor = None
        self._loaded = False

    def _resolve_device(self, device: str) -> str:
        if device == "auto":
            if torch.cuda.is_available():
                return "cuda"
            elif hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
                return "mps"
            return "cpu"
        return device

    def load(self):
        """Load model into memory."""
        if self._loaded:
            return

        print(f"Loading {self.model_name} on {self.device}...")
        t0 = time.time()

        self.processor = AutoProcessor.from_pretrained(
            self.model_name, trust_remote_code=True
        )
        self.model = AutoModelForCausalLM.from_pretrained(
            self.model_name,
            trust_remote_code=True,
            torch_dtype=torch.float16 if self.device == "cuda" else torch.float32,
        ).to(self.device)

        self._loaded = True
        print(f"Model loaded in {time.time() - t0:.1f}s")

    def caption(self, image: Image.Image, task: str = "<DETAILED_CAPTION>") -> dict:
        """Run Florence-2 captioning on an image."""
        if not self._loaded:
            self.load()

        inputs = self.processor(text=task, images=image, return_tensors="pt").to(self.device)

        with torch.no_grad():
            generated_ids = self.model.generate(
                input_ids=inputs["input_ids"],
                pixel_values=inputs["pixel_values"],
                max_new_tokens=1024,
                num_beams=3,
            )

        generated_text = self.processor.batch_decode(generated_ids, skip_special_tokens=False)[0]
        result = self.processor.post_process_generation(
            generated_text, task=task, image_size=(image.width, image.height)
        )

        return result

    def analyze(self, image: Image.Image) -> dict:
        """Full analysis: caption + objects + OCR → structured JSON."""
        # Step 1: Detailed caption
        caption_result = self.caption(image, "<DETAILED_CAPTION>")
        caption_text = caption_result.get("<DETAILED_CAPTION>", "")

        # Step 2: Object detection
        od_result = self.caption(image, "<OD>")
        od_data = od_result.get("<OD>", {})
        objects = od_data.get("labels", []) if isinstance(od_data, dict) else []

        # Step 3: OCR
        ocr_result = self.caption(image, "<OCR>")
        ocr_text = ocr_result.get("<OCR>", "")
        if isinstance(ocr_text, dict):
            ocr_text = ocr_text.get("text", "")

        # Build tags from detected objects
        tags = []
        seen = set()
        for obj in objects:
            if isinstance(obj, str) and obj not in seen:
                seen.add(obj)
                tags.append({"name": obj, "confidence": 0.85})

        # Build entities
        entities = []
        for obj in objects[:10]:  # Limit to 10
            if isinstance(obj, str):
                entities.append({
                    "type": "object",
                    "name": obj,
                    "confidence": 0.85
                })

        return {
            "caption": caption_text,
            "tags": tags,
            "entities": entities,
            "ocr_text": ocr_text if ocr_text and ocr_text.strip() else None,
        }


# ── HTTP Server ───────────────────────────────────────────────────

captioner: Florence2Captioner = None


class CaptionHandler(BaseHTTPRequestHandler):
    """HTTP request handler for /caption endpoint."""

    def do_POST(self):
        if self.path == "/caption":
            self._handle_caption()
        elif self.path == "/health":
            self._handle_health()
        else:
            self._respond(404, {"error": "not found"})

    def do_GET(self):
        if self.path == "/health":
            self._handle_health()
        else:
            self._respond(404, {"error": "not found"})

    def _handle_caption(self):
        try:
            content_length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(content_length)
            data = json.loads(body)

            image_b64 = data.get("image_base64", "")
            if not image_b64:
                self._respond(400, {"error": "missing image_base64"})
                return

            # Decode image
            image_bytes = base64.b64decode(image_b64)
            image = Image.open(io.BytesIO(image_bytes)).convert("RGB")

            # Run analysis
            t0 = time.time()
            result = captioner.analyze(image)
            elapsed = time.time() - t0

            print(f"[caption] {image.size[0]}x{image.size[1]} → {elapsed:.1f}s, "
                  f"caption_len={len(result['caption'])}")

            self._respond(200, {
                "caption": result["caption"],
                "tags": result["tags"],
                "entities": result["entities"],
                "ocr_text": result["ocr_text"],
                "processing_time_ms": int(elapsed * 1000),
            })

        except Exception as e:
            print(f"[caption] error: {e}")
            self._respond(500, {"error": str(e)})

    def _handle_health(self):
        self._respond(200, {
            "status": "ok",
            "model": captioner.model_name if captioner else None,
            "loaded": captioner._loaded if captioner else False,
            "device": captioner.device if captioner else None,
        })

    def _respond(self, status: int, data: dict):
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(json.dumps(data, ensure_ascii=False).encode("utf-8"))

    def log_message(self, format, *args):
        # Suppress default logging
        pass


def main():
    parser = argparse.ArgumentParser(description="Florence-2 Captioning Server")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Server port")
    parser.add_argument("--model", default=DEFAULT_MODEL, help="Florence-2 model name")
    parser.add_argument("--device", default="auto", help="Device: auto, cuda, cpu, mps")
    args = parser.parse_args()

    global captioner
    captioner = Florence2Captioner(args.model, args.device)
    captioner.load()

    server = HTTPServer(("127.0.0.1", args.port), CaptionHandler)
    print(f"Florence-2 Caption Server listening on http://127.0.0.1:{args.port}")
    print(f"Model: {args.model} | Device: {captioner.device}")

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...")
        server.shutdown()


if __name__ == "__main__":
    main()
