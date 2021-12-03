import logging
import traceback
from pathlib import Path
from typing import Callable, List, Tuple

from . import BenchmarkInstance
from .database import BenchmarkResultRecord, Database, create_identifier_key
from .executor import BenchmarkExecutor
from .identifier import BenchmarkIdentifier
from .result import BenchmarkResult, Failure, Success, Timeout

DEFAULT_TIMEOUT_S = 180.0


class BenchmarkRunner:
    def __init__(
        self,
        database: Database,
        workdir: Path,
        materialize_fn: Callable[
            [BenchmarkIdentifier, Path], Tuple[BenchmarkIdentifier, BenchmarkInstance]
        ],
        exit_on_error=True
    ):
        self.database = database
        self.executor = BenchmarkExecutor()
        self.workdir = workdir.absolute()
        self.workdir.mkdir(parents=True, exist_ok=True)
        self.materialize_fn = materialize_fn
        self.exit_on_error = exit_on_error

    def compute(self, identifiers: List[BenchmarkIdentifier]):
        not_completed = self._skip_completed(identifiers)
        # Materialize instances immediately to find potential errors sooner
        instances = [
            self.materialize_fn(identifier, self.workdir)
            for identifier in not_completed
        ]

        logging.info(
            f"Skipping {len(identifiers) - len(not_completed)} out of {len(identifiers)} "
            f"benchmark(s)"
        )

        for (identifier, instance) in instances:
            logging.info(f"Executing benchmark {identifier}")
            timeout = identifier.timeout() or DEFAULT_TIMEOUT_S

            try:
                result = self.executor.execute(instance, timeout_s=timeout)
            except BaseException as e:
                tb = traceback.format_exc()
                logging.error(f"Unexpected benchmarking error has occurred: {tb}")
                result = Failure(e, tb)

            self._handle_result(identifier, result)

            yield (identifier, instance, result)

    def save(self):
        self.database.save()

    def _skip_completed(
        self, identifiers: List[BenchmarkIdentifier]
    ) -> List[BenchmarkIdentifier]:
        not_completed = []
        visited = set()
        for identifier in identifiers:
            key = create_identifier_key(identifier)
            if key in visited:
                raise Exception(f"Duplicated identifier: {identifier} in {identifiers}")
            visited.add(key)
            if not self.database.has_record_for(identifier):
                not_completed.append(identifier)
        return not_completed

    def _handle_result(self, identifier: BenchmarkIdentifier, result: BenchmarkResult):
        key = identifier
        duration = None

        if isinstance(result, Failure):
            logging.error(f"Benchmark {key} has failed: {result.traceback}")
            if self.exit_on_error:
                raise Exception(f"""Benchmark {identifier} has failed: {result}
You can find details in {identifier.workdir()}""")
        elif isinstance(result, Timeout):
            logging.info(f"Benchmark {key} has timeouted after {result.timeout}s")
        elif isinstance(result, Success):
            logging.info(f"Benchmark {key} has finished in {result.duration}s")
            duration = result.duration
        else:
            raise Exception(f"Unknown benchmark result {result}")

        self.database.store(identifier, BenchmarkResultRecord(duration=duration))
