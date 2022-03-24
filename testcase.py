def first_function():
    return 1


def second_function():
    return 2


def third_function():
    return 3


def fourth_function():
    return 4


def fifth_function():
    return 5


class SomeClass:
    def __init__(self):
        self.some_variable = 1

    def __str__(self):
        return "sup"

    def first_method(self):
        return 1

    def second_method(self):
        return 2


if __name__ == "__main__":
    first_function()
    second_function()
    fifth_function()
    t = SomeClass()
    t.first_method()
