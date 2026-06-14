<?php

namespace Sample;

use Sample\Support\Logger;

class UserService
{
    public function findById(int $id): string
    {
        return $this->render($this->loadUser($id));
    }

    public function renderUser(int $id): string
    {
        return $this->findById($id);
    }

    private function loadUser(int $id): string
    {
        return "user#" . $id;
    }

    private function render(string $user): string
    {
        return "[" . $user . "]";
    }
}
