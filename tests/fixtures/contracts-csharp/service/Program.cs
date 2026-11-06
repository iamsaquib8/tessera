using System;

public class OrderHandlers
{
    public static string CreateOrder() => "created";
    public static string DeleteOrder() => "deleted";
}

public class ItemHandlers
{
    public static string UpdateItem() => "updated";
}

var app = WebApplication.Create();

// Top-level minimal API routes:
app.MapGet("/users/{id}", (int id) => "user");
app.MapPost("/orders", OrderHandlers.CreateOrder);
app.MapPut("/items/{id}", ItemHandlers.UpdateItem);

// Group variable:
var api = app.MapGroup("/api");
api.MapDelete("/orders/{id}", OrderHandlers.DeleteOrder);
api.MapGet("/status", () => "ok");

// Chained MapGroup:
app.MapGroup("/v2").MapGet("/ping", () => "pong");

// Lookalikes that must not match as route facts:
var cache = new Cache();
cache.MapGet("/x", () => "cached");
var mapper = new Mapper();
mapper.MapPost("/not-a-route", 123);

public class Cache
{
    public void MapGet(string path, Func<string> f) {}
}

public class Mapper
{
    public void MapPost(string path, int x) {}
}
