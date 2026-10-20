from functools import cache

from . import util
from .models import User


def load(id):
    user = User(util.load(id))
    return user.display()


@cache
def cached_load(id):
    return load(id)
