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


@contextmanager
def random_kv():
    k = generate_random_string(10)
    v = generate_random_string(10)

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


if __name__ == "__main__":
    unittest.main()
