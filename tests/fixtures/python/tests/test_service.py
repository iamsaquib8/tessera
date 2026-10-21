from app.service import load


def test_load():
    assert load("a")
