"""Small declarative OpenUSD assets owned by the external fixture."""

from __future__ import annotations


ENVIRONMENT_USDA = b"""#usda 1.0
(
    defaultPrim = "Environment"
    metersPerUnit = 1
    upAxis = "Z"
)

def Xform "Environment"
{
    def Mesh "Ground"
    {
        point3f[] points = [(-40, -40, 0), (40, -40, 0), (40, 40, 0), (-40, 40, 0)]
        int[] faceVertexCounts = [4]
        int[] faceVertexIndices = [0, 1, 2, 3]
        uniform token subdivisionScheme = "none"
        color3f[] primvars:displayColor = [(0.08, 0.12, 0.16)]
    }
}
"""


PROTOTYPE_USDA = b"""#usda 1.0
(
    defaultPrim = "SyntheticVehicle"
    metersPerUnit = 1
    upAxis = "Z"
)

def Xform "SyntheticVehicle"
{
    def Mesh "Body"
    {
        point3f[] points = [
            (-0.8, -0.4, -0.2), (0.8, -0.4, -0.2),
            (0.8, 0.4, -0.2), (-0.8, 0.4, -0.2),
            (-0.8, -0.4, 0.2), (0.8, -0.4, 0.2),
            (0.8, 0.4, 0.2), (-0.8, 0.4, 0.2)
        ]
        int[] faceVertexCounts = [4, 4, 4, 4, 4, 4]
        int[] faceVertexIndices = [
            0, 1, 2, 3, 4, 7, 6, 5,
            0, 4, 5, 1, 1, 5, 6, 2,
            2, 6, 7, 3, 4, 0, 3, 7
        ]
        uniform token subdivisionScheme = "none"
        color3f[] primvars:displayColor = [(0.05, 0.75, 0.95)]
    }
}
"""
