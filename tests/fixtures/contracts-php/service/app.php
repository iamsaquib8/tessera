<?php

use Slim\Factory\AppFactory;
use Slim\Routing\RouteCollectorProxy;

$app = AppFactory::create();

class OrderController {
    public function create($request, $response) {
        return $response;
    }

    public function delete($request, $response) {
        return $response;
    }
}

class ItemController {
    public function update($request, $response) {
        return $response;
    }
}

function healthCheck($request, $response) {
    return $response;
}

function pingHandler($request, $response) {
    return $response;
}

// Closure handler
$app->get('/users/{id}', function ($request, $response, array $args) {
    return $response;
});

// [Class::class, 'method'] handler
$app->post('/orders', [OrderController::class, 'create']);

// 'Class:method' handler
$app->put('/items/{id}', 'ItemController:update');

// Arrow function handler
$app->patch('/users/{id}', fn ($req, $res) => $res);

// Identifier/plain string handler
$app->options('/health', 'healthCheck');

// Grouped routes with prefix:
$app->group('/api', function (RouteCollectorProxy $group) {
    $group->delete('/orders/{id}', 'OrderController:delete');
    $group->get('/status', function ($request, $response) {
        return $response;
    });
});

// Grouped routes with arrow function:
$app->group('/v2', fn ($group) => $group->get('/ping', 'pingHandler'));

// Look-alikes that must not match as route facts:
$cache->get('/x');
$store->get('/api/status', 'fallback');
