import random
import string
import subprocess
import unittest
from contextlib import contextmanager


def run(args):
    return subprocess.run(args, capture_output=True, text=True, check=True)


def generate_random_string(length):
    """Generates a general-purpose, random alphanumeric string of a specified length."""
    characters = string.ascii_letters + string.digits
    # random.choices returns a list, which we join into a single string
    return "".join(random.choices(characters, k=length))


def get(key):
    return run(["blade", "get", key])


def set(key, value):
    return run(["blade", "set", key, value])


def delete(key):
    return run(["blade", "delete", key])


def list():
    return run(["blade", "list"])


def list_with_namespace(ns):
    return run(["blade", "list", ns])


@contextmanager
def random_kv(ns=None):
    k = generate_random_string(10)
    v = generate_random_string(10)

    if ns:
        k = k + "@" + ns
        ns = None

    try:
        yield k, v
    finally:
        delete(k)


class TestBlade(unittest.TestCase):
    def test_get_and_set(self):
        with random_kv() as (key, value):
            set_out = set(key, value)

            self.assertEqual(set_out.returncode, 0)

            get_out = get(key)

            self.assertEqual(get_out.returncode, 0)
            self.assertEqual(get_out.stdout, value + "\n")

    def test_get_and_set_with_namespaces(self):
        try:
            key1 = "key@ns1"
            value1 = "value1"

            key2 = "key@ns2"
            value2 = "other value"

            set_out1 = set(key1, value1)

            self.assertEqual(set_out1.returncode, 0)

            get_out1 = get(key1)

            self.assertEqual(get_out1.returncode, 0)
            self.assertEqual(get_out1.stdout, value1 + "\n")

            set_out2 = set(key2, value2)

            self.assertEqual(set_out2.returncode, 0)

            get_out2 = get(key2)

            self.assertEqual(get_out2.returncode, 0)
            self.assertEqual(get_out2.stdout, value2 + "\n")

            self.assertNotEqual(get_out1.stdout, get_out2.stdout)

        finally:
            delete(key1)
            delete(key2)

    def test_delete(self):
        with random_kv() as (key, value):
            set_out = set(key, value)
            self.assertEqual(set_out.returncode, 0)

            get_out = get(key)
            self.assertEqual(get_out.returncode, 0)
            self.assertEqual(get_out.stdout, value + "\n")

            delete_out = delete(key)
            self.assertEqual(delete_out.returncode, 0)
            self.assertEqual(delete_out.returncode, 0)
            self.assertEqual(delete_out.stdout, "")

    def test_list(self):
        self.maxDiff = None

        with (
            random_kv() as (key1, value1),
            random_kv() as (key2, value2),
            random_kv() as (key3, value3),
        ):
            set_out = set(key1, value1)
            self.assertEqual(set_out.returncode, 0)

            set_out2 = set(key2, value2)
            self.assertEqual(set_out2.returncode, 0)

            set_out3 = set(key3, value3)
            self.assertEqual(set_out3.returncode, 0)

            list_out = list()

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
            random_kv("ns1") as (key1, value1),
            random_kv("ns2") as (key2, value2),
            random_kv("ns2") as (key3, value3),
        ):
            set_out = set(key1, value1)
            self.assertEqual(set_out.returncode, 0)

            set_out2 = set(key2, value2)
            self.assertEqual(set_out2.returncode, 0)

            set_out3 = set(key3, value3)
            self.assertEqual(set_out3.returncode, 0)

            list_out = list_with_namespace("ns1")

            self.assertEqual(list_out.returncode, 0)

            self.assertEqual(
                list_out.stdout,
                "\n".join(
                    [
                        "\t".join([key1.rstrip("@ns1"), value1]),
                    ]
                )
                + "\n",
            )

            list_out2 = list_with_namespace("ns2")

            self.assertEqual(list_out2.returncode, 0)

            self.assertEqual(
                list_out2.stdout,
                "\n".join(
                    [
                        "\t".join([key3.rstrip("@ns2"), value3]),
                        "\t".join([key2.rstrip("@ns2"), value2]),
                    ]
                )
                + "\n",
            )


if __name__ == "__main__":
    unittest.main()
