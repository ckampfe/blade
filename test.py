import os
import random
import string
import subprocess
import tempfile
import unittest
from contextlib import contextmanager


def run(db, args):
    my_env = os.environ.copy()
    my_env["DB_LOCATION"] = db
    return subprocess.run(args, capture_output=True, text=True, check=True, env=my_env)


def generate_random_string(length):
    """Generates a general-purpose, random alphanumeric string of a specified length."""
    characters = string.ascii_letters + string.digits
    # random.choices returns a list, which we join into a single string
    return "".join(random.choices(characters, k=length))


def get(db, key):
    return run(db, ["blade", "get", key])


def set(db, key, value):
    return run(db, ["blade", "set", key, value])


def set_from_stdin(db, key, value):
    my_env = os.environ.copy()
    my_env["DB_LOCATION"] = db
    return subprocess.run(
        ["blade", "set", key],
        capture_output=True,
        text=True,
        check=True,
        env=my_env,
        input=value,
    )


def set_from_file_input(db, key, file):
    my_env = os.environ.copy()
    my_env["DB_LOCATION"] = db
    return subprocess.run(
        ["blade", "set", key],
        capture_output=True,
        text=True,
        check=True,
        env=my_env,
        stdin=file,
    )


def delete(db, key):
    return run(db, ["blade", "delete", key])


def list(db):
    return run(db, ["blade", "list"])


def list_with_namespace(db, ns):
    return run(db, ["blade", "list", ns])


def dump_config(db):
    return run(db, ["blade", "dump-config"])


@contextmanager
def random_kv(ns=None):
    k = generate_random_string(10)
    v = generate_random_string(10)

    if ns:
        k = k + "@" + ns
        ns = None

    yield k, v


@contextmanager
def test_db():
    with tempfile.TemporaryDirectory() as tmpdirname:
        db = tmpdirname + "/test.db"
        yield db


class TestBlade(unittest.TestCase):
    def test_get_and_set(self):
        with test_db() as db, random_kv() as (key, value):
            set_out = set(db, key, value)

            self.assertEqual(set_out.returncode, 0)

            get_out = get(db, key)

            self.assertEqual(get_out.returncode, 0)
            self.assertEqual(get_out.stdout, value + "\n")

    def test_get_and_set_from_stdin(self):
        with test_db() as db, random_kv() as (key, value):
            set_out = set_from_stdin(db, key, value)

            self.assertEqual(set_out.returncode, 0)

            get_out = get(db, key)

            self.assertEqual(get_out.returncode, 0)
            self.assertEqual(get_out.stdout, value + "\n")

    def test_get_and_set_from_stdin_fd(self):
        with (
            test_db() as db,
            random_kv() as (key, value),
            tempfile.NamedTemporaryFile() as file,
        ):
            file_contents = "hello world"
            file.write(bytes(file_contents, "utf-8"))
            file.seek(0)

            set_out = set_from_file_input(db, key, file)

            self.assertEqual(set_out.returncode, 0)

            get_out = get(db, key)

            self.assertEqual(get_out.returncode, 0)
            self.assertEqual(get_out.stdout, file_contents + "\n")

    def test_get_and_set_with_namespaces(self):
        with test_db() as db:
            key1 = "key@ns1"
            value1 = "value1"

            key2 = "key@ns2"
            value2 = "other value"

            set_out1 = set(db, key1, value1)

            self.assertEqual(set_out1.returncode, 0)

            get_out1 = get(db, key1)

            self.assertEqual(get_out1.returncode, 0)
            self.assertEqual(get_out1.stdout, value1 + "\n")

            set_out2 = set(db, key2, value2)

            self.assertEqual(set_out2.returncode, 0)

            get_out2 = get(db, key2)

            self.assertEqual(get_out2.returncode, 0)
            self.assertEqual(get_out2.stdout, value2 + "\n")

            self.assertNotEqual(get_out1.stdout, get_out2.stdout)

    def test_delete(self):
        with test_db() as db, random_kv() as (key, value):
            set_out = set(db, key, value)
            self.assertEqual(set_out.returncode, 0)

            get_out = get(db, key)
            self.assertEqual(get_out.returncode, 0)
            self.assertEqual(get_out.stdout, value + "\n")

            delete_out = delete(db, key)
            self.assertEqual(delete_out.returncode, 0)
            self.assertEqual(delete_out.returncode, 0)
            self.assertEqual(delete_out.stdout, "")

    def test_list(self):
        self.maxDiff = None

        with (
            test_db() as db,
            random_kv() as (key1, value1),
            random_kv() as (key2, value2),
            random_kv() as (key3, value3),
        ):
            set_out = set(db, key1, value1)
            self.assertEqual(set_out.returncode, 0)

            set_out2 = set(db, key2, value2)
            self.assertEqual(set_out2.returncode, 0)

            set_out3 = set(db, key3, value3)
            self.assertEqual(set_out3.returncode, 0)

            list_out = list(db)

            self.assertEqual(list_out.returncode, 0)

            self.assertEqual(
                list_out.stdout,
                "\n".join(
                    [
                        "\t".join([key3, value3]),
                        "\t".join([key2, value2]),
                        "\t".join([key1, value1]),
                    ]
                )
                + "\n",
            )

    def test_list_with_namespaces(self):
        self.maxDiff = None

        with (
            test_db() as db,
            random_kv("ns1") as (key1, value1),
            random_kv("ns2") as (key2, value2),
            random_kv("ns2") as (key3, value3),
        ):
            set_out = set(db, key1, value1)
            self.assertEqual(set_out.returncode, 0)

            set_out2 = set(db, key2, value2)
            self.assertEqual(set_out2.returncode, 0)

            set_out3 = set(db, key3, value3)
            self.assertEqual(set_out3.returncode, 0)

            list_out = list_with_namespace(db, "ns1")

            self.assertEqual(list_out.returncode, 0)

            self.assertEqual(
                list_out.stdout,
                "\n".join(
                    [
                        "\t".join([key1.removesuffix("@ns1"), value1]),
                    ]
                )
                + "\n",
            )

            list_out2 = list_with_namespace(db, "ns2")

            self.assertEqual(list_out2.returncode, 0)

            self.assertEqual(
                list_out2.stdout,
                "\n".join(
                    [
                        "\t".join([key3.removesuffix("@ns2"), value3]),
                        "\t".join([key2.removesuffix("@ns2"), value2]),
                    ]
                )
                + "\n",
            )

    def test_dump_config(self):
        with test_db() as db:
            dump_config_out = dump_config(db)
            self.assertIn("db_location = ", dump_config_out.stdout)
            self.assertIn("blade.db", dump_config_out.stdout)
            self.assertIn('sqlite_synchronous_mode = "normal"', dump_config_out.stdout)
            self.assertIn("sqlite_busy_timeout_ms = 5000", dump_config_out.stdout)


if __name__ == "__main__":
    unittest.main()
