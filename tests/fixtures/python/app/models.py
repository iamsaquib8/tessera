from app.util import format_name


class User:
    def __init__(self, name):
        self.name = name

    def display(self):
        return format_name(self.name)
