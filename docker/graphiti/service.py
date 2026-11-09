"""Jocasta's Graphiti service (5.9, 0334): a small HTTP API over graphiti-core 0.30.2 and FalkorDB.

POST /episodes  {"group_id", "episodes": [{"name", "body", "source", "reference_time"}],
                 "llm": {"base_url", "api_key"}}                     -> {"added": n}
POST /search    {"group_id", "query", "limit"}                       -> {"facts": [...]}
GET  /health                                                         -> {"ok": true}

Every request names exactly one group id (0277): a Jocasta project id; FalkorDB keeps each group in its own graph.
Extraction on add calls the OpenAI-compatible endpoint the companion serves (0334, 0390), given per request,
with the model names `main` and `small`.
Embeddings run here (sentence-transformers); search reranks with RRF and never calls a model.

Access (0430): the Host header must name 127.0.0.1 or localhost (no DNS rebinding); `/episodes` and `/search` need
`Authorization: Bearer <GRAPHITI_TOKEN>`, the per-run token the companion passes in the compose environment (no token
set refuses them all), and a JSON body (`Content-Type: application/json`).
"""

import asyncio
import hmac
import json
import logging
import os
import threading
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from graphiti_core import Graphiti
from graphiti_core.cross_encoder.client import CrossEncoderClient
from graphiti_core.driver.falkordb_driver import FalkorDriver
from graphiti_core.embedder.client import EmbedderClient
from graphiti_core.helpers import validate_group_id
from graphiti_core.llm_client.config import LLMConfig, ModelSize
from graphiti_core.llm_client.openai_generic_client import OpenAIGenericClient
from graphiti_core.nodes import EpisodeType, EpisodicNode
from sentence_transformers import SentenceTransformer

logging.basicConfig(level=os.environ.get('LOG_LEVEL', 'INFO'))
log = logging.getLogger('jocasta-graphiti')

EMBEDDING_MODEL = os.environ['EMBEDDING_MODEL']
EMBEDDING_REVISION = os.environ.get('EMBEDDING_REVISION')
TOKEN = os.environ.get('GRAPHITI_TOKEN', '')
HOSTS = ('127.0.0.1', 'localhost')
FALKORDB_HOST = os.environ.get('FALKORDB_HOST', 'falkordb')
FALKORDB_PORT = int(os.environ.get('FALKORDB_PORT', '6379'))
PORT = int(os.environ.get('PORT', '8000'))
MAX_LIMIT = 50


class LocalEmbedder(EmbedderClient):
    """sentence-transformers on the CPU; the model is baked into the image."""

    def __init__(self) -> None:
        self.model = SentenceTransformer(EMBEDDING_MODEL, revision=EMBEDDING_REVISION)

    async def create(self, input_data):  # type: ignore[override]
        text = input_data if isinstance(input_data, str) else ' '.join(map(str, input_data))
        vector = await asyncio.to_thread(self.model.encode, text, normalize_embeddings=True)
        return vector.tolist()

    async def create_batch(self, input_data_list: list[str]) -> list[list[float]]:
        vectors = await asyncio.to_thread(self.model.encode, input_data_list, normalize_embeddings=True)
        return [v.tolist() for v in vectors]


class NoReranker(CrossEncoderClient):
    """Search uses RRF (EDGE_HYBRID_SEARCH_RRF); a cross-encoder rerank would be a model call, so there is none."""

    async def rank(self, query: str, passages: list[str]) -> list[tuple[str, float]]:
        raise RuntimeError('cross-encoder reranking is not available; search uses RRF')


MAIN_MODEL = 'main'
SMALL_MODEL = 'small'


class SizedClient(OpenAIGenericClient):
    """OpenAIGenericClient sends every prompt to `config.model`; this one sends Graphiti's small prompts to the small model."""

    def __init__(self, base_url: str, api_key: str) -> None:
        super().__init__(config=LLMConfig(api_key=api_key, model=MAIN_MODEL, small_model=SMALL_MODEL, base_url=base_url))
        small = LLMConfig(api_key=api_key, model=SMALL_MODEL, small_model=SMALL_MODEL, base_url=base_url)
        self.small = OpenAIGenericClient(config=small, client=self.client)

    async def _generate_response(self, messages, response_model=None, max_tokens=None, model_size=ModelSize.medium):  # type: ignore[override]
        if model_size == ModelSize.small:
            return await self.small._generate_response(messages, response_model, max_tokens, model_size)
        return await super()._generate_response(messages, response_model, max_tokens, model_size)


class Service:
    def __init__(self) -> None:
        self.loop = asyncio.new_event_loop()
        threading.Thread(target=self.loop.run_forever, daemon=True).start()
        self.embedder = LocalEmbedder()
        self.driver: FalkorDriver = self.run(self._driver())
        self.add_lock = asyncio.Lock()
        # Search never reaches the model; this client only satisfies Graphiti's constructor.
        self.searcher = self._graphiti(SizedClient('http://127.0.0.1:9/v1', 'unused'))

    async def _driver(self) -> FalkorDriver:
        return FalkorDriver(host=FALKORDB_HOST, port=FALKORDB_PORT)

    def _graphiti(self, client: OpenAIGenericClient) -> Graphiti:
        return Graphiti(graph_driver=self.driver, llm_client=client, embedder=self.embedder, cross_encoder=NoReranker())

    def run(self, coro):
        return asyncio.run_coroutine_threadsafe(coro, self.loop).result()

    async def add(self, group_id: str, episodes: list[dict], llm: dict) -> int:
        graphiti = self._graphiti(SizedClient(llm['base_url'], llm['api_key']))
        async with self.add_lock:
            for episode in episodes:
                await graphiti.add_episode(
                    name=str(episode['name']),
                    episode_body=str(episode['body']),
                    source_description=str(episode.get('source') or episode['name']),
                    reference_time=datetime.fromisoformat(str(episode['reference_time']).replace('Z', '+00:00')),
                    source=EpisodeType.text,
                    group_id=group_id,
                )
        return len(episodes)

    async def search(self, group_id: str, query: str, limit: int) -> list[dict]:
        edges = await self.searcher.search(query, group_ids=[group_id], num_results=limit)
        edges = [edge for edge in edges if edge.group_id == group_id]
        iso = lambda value: value.isoformat() if value else None  # noqa: E731
        # Each fact's source (0484): the names of the episodes it came from (`<kind> <session>`), read in the group's
        # own graph.
        uuids = sorted({uuid for edge in edges for uuid in edge.episodes})
        names: dict[str, str] = {}
        if uuids:
            episodes = await EpisodicNode.get_by_uuids(self.driver.clone(database=group_id), uuids)
            names = {episode.uuid: episode.name for episode in episodes}
        return [
            {
                'fact': edge.fact,
                'name': edge.name,
                'valid_at': iso(edge.valid_at),
                'invalid_at': iso(edge.invalid_at),
                'created_at': iso(edge.created_at),
                'sources': [names[uuid] for uuid in edge.episodes if uuid in names],
            }
            for edge in edges
        ]


def group_of(body: dict) -> str:
    group_id = body.get('group_id')
    if not isinstance(group_id, str) or not group_id:
        raise ValueError('group_id is required')
    validate_group_id(group_id)
    return group_id


class Handler(BaseHTTPRequestHandler):
    service: Service

    def _reply(self, status: int, payload: dict) -> None:
        data = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header('content-type', 'application/json')
        self.send_header('content-length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _host_ok(self) -> bool:
        host = (self.headers.get('host') or '').strip().lower()
        name = host.rsplit(':', 1)[0] if host.count(':') == 1 else host
        return name in HOSTS

    def do_GET(self) -> None:  # noqa: N802
        if not self._host_ok():
            self._reply(403, {'error': 'host not allowed'})
        elif self.path == '/health':
            self._reply(200, {'ok': True})
        else:
            self._reply(404, {'error': 'not found'})

    def do_POST(self) -> None:  # noqa: N802
        if not self._host_ok():
            self._reply(403, {'error': 'host not allowed'})
            return
        auth = self.headers.get('authorization') or ''
        given = auth[7:] if auth.startswith('Bearer ') else ''
        if not TOKEN or not hmac.compare_digest(given.encode(), TOKEN.encode()):
            self._reply(401, {'error': 'invalid token'})
            return
        content_type = (self.headers.get('content-type') or '').split(';', 1)[0].strip().lower()
        if content_type != 'application/json':
            self._reply(415, {'error': 'JSON bodies only'})
            return
        try:
            body = json.loads(self.rfile.read(int(self.headers.get('content-length') or 0)) or b'{}')
            if not isinstance(body, dict):
                raise ValueError('the body must be a JSON object')
            group_id = group_of(body)
            if self.path == '/episodes':
                episodes = body.get('episodes')
                llm = body.get('llm')
                if not isinstance(episodes, list) or not isinstance(llm, dict):
                    raise ValueError('episodes and llm are required')
                added = self.service.run(self.service.add(group_id, episodes, llm))
                self._reply(200, {'added': added})
            elif self.path == '/search':
                query = str(body.get('query') or '')
                limit = max(1, min(int(body.get('limit') or 10), MAX_LIMIT))
                facts = self.service.run(self.service.search(group_id, query, limit))
                self._reply(200, {'facts': facts})
            else:
                self._reply(404, {'error': 'not found'})
        except (ValueError, KeyError, TypeError) as error:
            self._reply(400, {'error': str(error)})
        except Exception as error:  # noqa: BLE001
            log.exception('request failed')
            self._reply(500, {'error': f'{type(error).__name__}: {error}'})

    def log_message(self, format: str, *args) -> None:  # noqa: A002
        log.info('%s %s', self.address_string(), format % args)


def main() -> None:
    Handler.service = Service()
    log.info('listening on %s', PORT)
    ThreadingHTTPServer(('0.0.0.0', PORT), Handler).serve_forever()


if __name__ == '__main__':
    main()
