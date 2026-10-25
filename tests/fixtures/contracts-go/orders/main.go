package main

import (
	"github.com/gin-gonic/gin"
	"github.com/labstack/echo/v4"
)

func main() {
	r := gin.Default()
	r.GET("/orders/:id", getOrder)
	api := r.Group("/api")
	api.POST("/orders", createOrder)
	r.Run()
}

func admin() {
	e := echo.New()
	e.GET("/items/:id", getItem)
	g := e.Group("/admin")
	g.DELETE("/items/:id", deleteItem)
}

func getOrder(c *gin.Context)         { c.JSON(200, c.Param("id")) }
func createOrder(c *gin.Context)      {}
func getItem(c echo.Context) error    { return nil }
func deleteItem(c echo.Context) error { return nil }
